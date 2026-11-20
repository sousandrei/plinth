use std::{env, process};

use serde_json::json;

use super::*;

const SPACE: &str = "personal";
const RECORD: &str = "transaction-1";

fn edit(store: &mut Store, timestamp: i64, name: &str, value: &str) {
    store
        .patch(
            SPACE,
            RECORD,
            timestamp,
            &[(name.into(), json!(value))].into(),
        )
        .unwrap();
}

fn exchange(left: &mut Store, right: &mut Store) {
    let mut l = Session::new(SPACE);
    let mut r = Session::new(SPACE);
    for _ in 0..20 {
        let left_message = left.next_message(&mut l).unwrap();
        if let Some(bytes) = &left_message {
            right.receive(&mut r, bytes).unwrap();
        }
        let right_message = right.next_message(&mut r).unwrap();
        if let Some(bytes) = &right_message {
            left.receive(&mut l, bytes).unwrap();
        }
        if left_message.is_none() && right_message.is_none() {
            return;
        }
    }
    panic!("sync did not finish");
}

fn peers() -> (Store, Store) {
    let mut left = Store::open(":memory:").unwrap();
    let mut right = Store::open(":memory:").unwrap();
    edit(&mut left, 10, "description", "Shopping");
    edit(&mut left, 10, "category", "Other");
    exchange(&mut left, &mut right);
    (left, right)
}

fn assert_field(store: &Store, name: &str, expected: &str) {
    assert_eq!(
        store.snapshot(SPACE).unwrap()[RECORD][name],
        json!(expected)
    );
}

#[test]
fn offline_edits_to_different_fields_merge() {
    let (mut left, mut right) = peers();
    edit(&mut left, 20, "description", "Weekly shopping");
    edit(&mut right, 20, "category", "Groceries");
    right
        .patch(
            "other-space",
            RECORD,
            40,
            &[("category".into(), json!("Travel"))].into(),
        )
        .unwrap();
    exchange(&mut left, &mut right);

    assert_field(&left, "description", "Weekly shopping");
    assert_field(&left, "category", "Groceries");
    assert_eq!(
        left.snapshot(SPACE).unwrap(),
        right.snapshot(SPACE).unwrap()
    );
    assert!(left.snapshot("other-space").unwrap().is_empty());
}

#[test]
fn concurrent_edits_follow_timestamps_even_if_older_edit_arrives_last() {
    let (mut early, mut late) = peers();
    let mut early_first = Store::open(":memory:").unwrap();
    let mut late_first = Store::open(":memory:").unwrap();
    edit(&mut early, 20, "category", "Home");
    edit(&mut late, 30, "category", "Groceries");
    exchange(&mut early, &mut early_first);
    exchange(&mut late, &mut late_first);
    assert_field(&early_first, "category", "Home");
    assert_field(&late_first, "category", "Groceries");
    exchange(&mut early, &mut late_first);
    exchange(&mut late, &mut early_first);
    assert_field(&late_first, "category", "Groceries");
    assert_eq!(
        early_first.snapshot(SPACE).unwrap(),
        late_first.snapshot(SPACE).unwrap()
    );

    let doc = load(&late_first.db, SPACE).unwrap();
    let key = serde_json::to_string(&(RECORD, "category")).unwrap();
    assert_eq!(doc.get_all(ROOT, key).unwrap().len(), 2);
}

#[test]
fn equal_timestamps_converge_and_an_edit_that_saw_both_takes_effect() {
    let (mut left, mut right) = peers();
    edit(&mut left, 20, "category", "Groceries");
    edit(&mut right, 20, "category", "Home");
    exchange(&mut left, &mut right);
    assert_eq!(
        left.snapshot(SPACE).unwrap(),
        right.snapshot(SPACE).unwrap()
    );

    edit(&mut left, 15, "category", "Household");
    exchange(&mut left, &mut right);
    assert_field(&right, "category", "Household");
}

#[test]
fn failed_persistence_does_not_acknowledge_and_retry_is_idempotent() {
    let mut left = Store::open(":memory:").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("sync.sqlite");
    let mut right = Store::open(&db).unwrap();
    let mut l = Session::new(SPACE);
    let mut r = Session::new(SPACE);

    let initial = left.next_message(&mut l).unwrap().unwrap();
    right.receive(&mut r, &initial).unwrap();
    let reply = right.next_message(&mut r).unwrap().unwrap();
    left.receive(&mut l, &reply).unwrap();
    edit(&mut left, 20, "category", "Groceries");
    let update = left.next_message(&mut l).unwrap().unwrap();

    right
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER fail_write BEFORE INSERT ON documents
         BEGIN SELECT RAISE(ABORT, 'simulated storage failure'); END;",
        )
        .unwrap();
    let previous = r.state.clone();
    assert!(right.receive_projected(&mut r, &update, |_| true).is_err());
    assert_eq!(r.state, previous);
    assert!(right.snapshot(SPACE).unwrap().is_empty());
    assert!(right.next_pending_edit().unwrap().is_none());

    right.db.execute_batch("DROP TRIGGER fail_write;").unwrap();
    assert!(matches!(
        right.receive_projected(&mut r, &update, |_| false),
        Err(Error::InvalidField)
    ));
    assert_eq!(r.state, previous);
    right.receive_projected(&mut r, &update, |_| true).unwrap();
    right.receive_projected(&mut r, &update, |_| true).unwrap();
    assert_eq!(
        right
            .db
            .query_row("SELECT COUNT(*) FROM documents", [], |row| row
                .get::<_, u32>(0))
            .unwrap(),
        1
    );

    drop(right);
    let mut right = Store::open(&db).unwrap();
    assert_field(&right, "category", "Groceries");
    let pending = right.next_pending_edit().unwrap().unwrap();
    assert_eq!(
        pending.fields,
        [("category".into(), json!("Groceries"))].into()
    );
    right.complete_edit(pending.sequence).unwrap();
    assert!(right.next_pending_edit().unwrap().is_none());
    exchange(&mut left, &mut right);
    assert_eq!(
        left.snapshot(SPACE).unwrap(),
        right.snapshot(SPACE).unwrap()
    );
}

#[test]
fn committed_changes_survive_process_exit_and_checkpoint() {
    const CHILD_DB: &str = "PLINTH_SYNC_TEST_CHILD_DB";
    if let Some(path) = env::var_os(CHILD_DB) {
        let mut store = Store::open(path).unwrap();
        edit(&mut store, 20, "category", "Groceries");
        edit(&mut store, 25, "description", "Weekly shopping");
        // Bypass destructors so reopening must recover the committed writes.
        process::exit(0);
    }

    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("sync.sqlite");
    let child = process::Command::new(env::current_exe().unwrap())
        .args([
            "--exact",
            "tests::committed_changes_survive_process_exit_and_checkpoint",
        ])
        .env(CHILD_DB, &db)
        .output()
        .unwrap();
    assert!(child.status.success(), "{:?}", child);

    let mut store = Store::open(&db).unwrap();
    assert_field(&store, "description", "Weekly shopping");
    assert_field(&store, "category", "Groceries");
    store.checkpoint(SPACE).unwrap();
    edit(&mut store, 30, "description", "Updated after checkpoint");
    drop(store);

    let mut store = Store::open(&db).unwrap();
    let mut peer = Store::open(":memory:").unwrap();
    exchange(&mut store, &mut peer);
    assert_field(&peer, "description", "Updated after checkpoint");
    assert_field(&peer, "category", "Groceries");
}

#[test]
fn staging_failure_rolls_back_the_document_and_pending_edit() {
    let mut store = Store::open(":memory:").unwrap();
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER fail_stage BEFORE INSERT ON pending_edits
             BEGIN SELECT RAISE(ABORT, 'simulated queue failure'); END;",
        )
        .unwrap();

    let fields = [("category".into(), json!("Groceries"))].into();
    assert!(store.stage_edit(SPACE, RECORD, 10, &fields).is_err());
    assert!(store.snapshot(SPACE).unwrap().is_empty());
    assert!(store.next_pending_edit().unwrap().is_none());
}

#[test]
fn queued_edits_keep_their_order_across_a_checkpoint() {
    let mut store = Store::open(":memory:").unwrap();
    for (timestamp, category) in [(10, "Groceries"), (20, "Household")] {
        store
            .stage_edit(
                SPACE,
                RECORD,
                timestamp,
                &[("category".into(), json!(category))].into(),
            )
            .unwrap();
    }
    store.checkpoint_all().unwrap();
    let after_checkpoint = store.db.total_changes();
    store.checkpoint_all().unwrap();
    assert_eq!(store.db.total_changes(), after_checkpoint);

    let first = store.next_pending_edit().unwrap().unwrap();
    assert_eq!(first.fields["category"], json!("Groceries"));
    store.complete_edit(first.sequence).unwrap();
    store.complete_edit(first.sequence).unwrap();

    let second = store.next_pending_edit().unwrap().unwrap();
    assert!(second.sequence > first.sequence);
    assert_eq!(second.fields["category"], json!("Household"));
    store.complete_edit(second.sequence).unwrap();
    assert!(store.next_pending_edit().unwrap().is_none());
    assert_field(&store, "category", "Household");
}

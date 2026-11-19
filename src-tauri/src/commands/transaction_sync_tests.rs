use std::{env, path::Path, process};

use plinth_sync::{Session, Store};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use super::{
    TransactionPatch, apply_pending_transaction_edit, apply_transaction_patch, update_in_preview,
};
use crate::{
    commands::spaces::ExportTransaction,
    db::{self, DbPool},
    error::AppError,
    sync_preview::SyncPreview,
};

const SPACE: &str = "personal";
const ID: &str = "transaction-1";

async fn open_finance_db(path: &Path) -> DbPool {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .unwrap()
}

async fn seed_finance_db(path: &Path) -> DbPool {
    let db = open_finance_db(path).await;
    sqlx::migrate!("./migrations").run(&db).await.unwrap();

    let date = "2026-01-01";
    let account = "checking";
    sqlx::query_file!("queries/spaces/create_space.sql", SPACE, "Personal", date)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query_file!(
        "queries/spaces/import_account.sql",
        account,
        "Checking",
        "SEK",
        "checking",
        "manual",
        "#000000",
        SPACE,
    )
    .execute(&db)
    .await
    .unwrap();
    import_sample_transaction(&db, "Other").await;
    db
}

async fn import_sample_transaction(db: &DbPool, category: &str) {
    let date = "2026-01-01";
    sqlx::query_file!(
        "queries/spaces/import_transaction.sql",
        ID,
        date,
        date,
        "",
        "Market",
        "SEK",
        -1500_i64,
        9000_i64,
        0_i64,
        "Keep this note",
        category,
        "checking",
    )
    .execute(db)
    .await
    .unwrap();
}

async fn find_transaction(db: &DbPool) -> Option<ExportTransaction> {
    sqlx::query_file_as!(
        ExportTransaction,
        "queries/spaces/export_transactions.sql",
        SPACE
    )
    .fetch_optional(db)
    .await
    .unwrap()
}

async fn read_transaction(db: &DbPool) -> ExportTransaction {
    find_transaction(db).await.unwrap()
}

fn exchange(left: &mut Store, right: &mut Store) {
    let mut left_session = Session::new(SPACE);
    let mut right_session = Session::new(SPACE);
    for _ in 0..20 {
        let left_message = left.next_message(&mut left_session).unwrap();
        if let Some(bytes) = &left_message {
            right.receive(&mut right_session, bytes).unwrap();
        }
        let right_message = right.next_message(&mut right_session).unwrap();
        if let Some(bytes) = &right_message {
            left.receive(&mut left_session, bytes).unwrap();
        }
        if left_message.is_none() && right_message.is_none() {
            return;
        }
    }
    panic!("the peers did not converge");
}

#[tokio::test]
async fn recovers_synced_transaction_patch_into_finance_db() {
    let temp = tempfile::tempdir().unwrap();
    let db = seed_finance_db(&temp.path().join("plinth.db")).await;
    let sync_path = temp.path().join("sync.sqlite");
    let persisted = sync_path.clone();
    tokio::task::spawn_blocking(move || {
        let mut local = Store::open(persisted).unwrap();
        let mut peer = Store::open(":memory:").unwrap();
        local
            .patch(SPACE, ID, 10, &[("approved".into(), json!(true))].into())
            .unwrap();
        peer.patch(
            SPACE,
            ID,
            20,
            &[("category".into(), json!("Groceries"))].into(),
        )
        .unwrap();
        exchange(&mut local, &mut peer);
        // Leave the finance database untouched, as if the app exited before applying the edits.
    })
    .await
    .unwrap();

    let before = read_transaction(&db).await;
    assert_eq!(before.approved, 0);
    assert_eq!(before.category, "Other");

    let patch: TransactionPatch = tokio::task::spawn_blocking(move || {
        let snapshot = Store::open(sync_path).unwrap().snapshot(SPACE).unwrap();
        serde_json::from_value(serde_json::to_value(&snapshot[ID]).unwrap()).unwrap()
    })
    .await
    .unwrap();

    assert!(matches!(
        apply_transaction_patch(&db, "another-space", ID, &patch).await,
        Err(AppError::NotFound(_))
    ));
    for _ in 0..2 {
        apply_transaction_patch(&db, SPACE, ID, &patch)
            .await
            .unwrap();
        let after = read_transaction(&db).await;
        assert_eq!(after.approved, 1);
        assert_eq!(after.category, "Groceries");
        assert_eq!(after.note, before.note);
        assert_eq!(after.amount, before.amount);
        assert_eq!(after.text, before.text);
    }

    let note_only = serde_json::from_value(json!({"note": "New note"})).unwrap();
    apply_transaction_patch(&db, SPACE, ID, &note_only)
        .await
        .unwrap();
    assert_eq!(read_transaction(&db).await.category, "Groceries");

    let clear_category = serde_json::from_value(json!({"category": null})).unwrap();
    apply_transaction_patch(&db, SPACE, ID, &clear_category)
        .await
        .unwrap();
    let after = read_transaction(&db).await;
    assert!(after.category.is_empty());
    assert_eq!(after.approved, 1);
    assert_eq!(after.note, "New note");
    db.close().await;
}

fn stage_category(store: &mut Store) {
    store
        .stage_edit(
            SPACE,
            ID,
            10,
            &[("category".into(), json!("Groceries"))].into(),
        )
        .unwrap();
}

#[tokio::test]
async fn replays_a_pending_edit_after_either_process_exit() {
    const CHILD_DIR: &str = "PLINTH_TEST_TRANSACTION_REPLAY_DIR";
    const CHILD_PHASE: &str = "PLINTH_TEST_TRANSACTION_REPLAY_PHASE";
    const ABRUPT_EXIT: i32 = 73;

    if let Some(path) = env::var_os(CHILD_DIR) {
        let dir = Path::new(&path);
        let mut store = Store::open(dir.join("sync.sqlite")).unwrap();
        stage_category(&mut store);
        match env::var(CHILD_PHASE).unwrap().as_str() {
            "before-finance-write" => process::exit(ABRUPT_EXIT),
            "after-finance-write" => {
                let db = open_finance_db(&dir.join("finance.sqlite")).await;
                let edit = store.next_pending_edit().unwrap().unwrap();
                apply_pending_transaction_edit(&db, &edit).await.unwrap();
                process::exit(ABRUPT_EXIT);
            }
            _ => panic!("unknown exit point"),
        }
    }

    for (phase, expected_before_replay) in [
        ("before-finance-write", "Other"),
        ("after-finance-write", "Groceries"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let finance_path = temp.path().join("finance.sqlite");
        seed_finance_db(&finance_path).await.close().await;
        let child = process::Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "commands::transactions::sync_tests::replays_a_pending_edit_after_either_process_exit",
            ])
            .env(CHILD_DIR, temp.path())
            .env(CHILD_PHASE, phase)
            .output()
            .unwrap();
        assert_eq!(child.status.code(), Some(ABRUPT_EXIT), "{phase}: {child:?}");

        let before = open_finance_db(&finance_path).await;
        let store = Store::open(temp.path().join("sync.sqlite")).unwrap();
        assert_eq!(
            read_transaction(&before).await.category,
            expected_before_replay
        );
        assert!(store.next_pending_edit().unwrap().is_some());
        before.close().await;

        let preview = SyncPreview::open(temp.path().to_owned()).await.unwrap();
        let db = db::init(&finance_path, &preview).await.unwrap();
        assert!(store.next_pending_edit().unwrap().is_none());
        let row = read_transaction(&db).await;
        assert_eq!(row.category, "Groceries");
        assert_eq!(row.note, "Keep this note");
        assert_eq!(row.approved, 0);
        db.close().await;
    }
}

#[tokio::test]
async fn replay_skips_transactions_removed_with_their_space() {
    let temp = tempfile::tempdir().unwrap();
    let finance_path = temp.path().join("finance.sqlite");
    let db = seed_finance_db(&finance_path).await;
    let mut store = Store::open(temp.path().join("sync.sqlite")).unwrap();
    stage_category(&mut store);
    sqlx::query_file!("queries/spaces/delete_space.sql", SPACE)
        .execute(&db)
        .await
        .unwrap();
    db.close().await;

    let preview = SyncPreview::open(temp.path().to_owned()).await.unwrap();
    let db = db::init(&finance_path, &preview).await.unwrap();
    assert!(store.next_pending_edit().unwrap().is_none());
    assert!(find_transaction(&db).await.is_none());
    db.close().await;
}

#[tokio::test]
async fn preview_category_edit_preserves_newer_bulk_and_import_writes() {
    let temp = tempfile::tempdir().unwrap();
    let finance_path = temp.path().join("finance.sqlite");
    seed_finance_db(&finance_path).await.close().await;
    let preview = SyncPreview::open(temp.path().to_owned()).await.unwrap();
    let db = db::init(&finance_path, &preview).await.unwrap();
    update_in_preview(
        &db,
        &preview,
        SPACE,
        ID,
        TransactionPatch {
            category: Some(Some("Groceries".into())),
            note: Some("Keep this note".into()),
            approved: Some(false),
        },
    )
    .await
    .unwrap();

    let store = Store::open(temp.path().join("sync.sqlite")).unwrap();
    assert!(store.next_pending_edit().unwrap().is_none());
    assert_eq!(
        store.snapshot(SPACE).unwrap()[ID],
        [("category".to_owned(), json!("Groceries"))].into()
    );
    assert_eq!(read_transaction(&db).await.category, "Groceries");

    preview
        .stage(
            SPACE.into(),
            ID.into(),
            [("category".into(), json!("Older"))].into(),
        )
        .await
        .unwrap();
    let ids_json = serde_json::to_string(&[ID]).unwrap();
    sqlx::query_file!(
        "queries/transactions/bulk_categorize.sql",
        "Travel",
        ids_json,
        SPACE
    )
    .execute(&db)
    .await
    .unwrap();
    assert_eq!(read_transaction(&db).await.category, "Travel");
    assert!(store.next_pending_edit().unwrap().is_none());

    preview
        .stage(
            SPACE.into(),
            ID.into(),
            [("category".into(), json!("Older again"))].into(),
        )
        .await
        .unwrap();
    import_sample_transaction(&db, "Imported").await;
    assert_eq!(read_transaction(&db).await.category, "Imported");
    assert!(store.next_pending_edit().unwrap().is_none());
    db.close().await;
    let restarted = db::init(&finance_path, &preview).await.unwrap();
    assert_eq!(read_transaction(&restarted).await.category, "Imported");
    restarted.close().await;
}

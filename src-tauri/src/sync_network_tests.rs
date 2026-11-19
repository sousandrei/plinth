use super::*;
use crate::{
    commands::{accounts::Account, spaces::ExportTransaction, transactions},
    db, sync_import,
};

async fn profile(directory: &Path, user: &str) -> (DbPool, SyncPreview, Arc<Network>, TcpListener) {
    let store = SyncPreview::open(directory.to_owned()).await.unwrap();
    let db = db::init(&directory.join("finance.sqlite"), &store)
        .await
        .unwrap();
    let date = "2026-01-01";
    sqlx::query_file!("queries/users/create_user.sql", user, user, date, date)
        .execute(&db)
        .await
        .unwrap();
    let (network, listener) = Network::open(directory, 0, None).await.unwrap();
    (db, store, network, listener)
}

async fn note(db: &DbPool, space: &str) -> String {
    sqlx::query_file!(
        "queries/transactions/get_transaction_edit_fields.sql",
        r#"["purchase"]"#,
        space
    )
    .fetch_one(db)
    .await
    .unwrap()
    .note
}

#[tokio::test]
#[ignore = "requires localhost TCP"]
async fn joins_a_shared_space_and_syncs_later_edits_automatically() {
    let source_dir = tempfile::tempdir().unwrap();
    let target_dir = tempfile::tempdir().unwrap();
    let (source_db, source_store, source, listener) = profile(source_dir.path(), "source").await;
    let (target_db, target_store, target, _unused_listener) =
        profile(target_dir.path(), "target").await;
    let space = spaces::create_space_for_user("source", "Household", &source_db)
        .await
        .unwrap();
    sqlx::query_file!(
        "queries/spaces/import_account.sql",
        "checking",
        "Checking",
        "SEK",
        "checking",
        "manual",
        "#222222",
        space
    )
    .execute(&source_db)
    .await
    .unwrap();
    let date = "2026-01-01";
    sqlx::query_file!(
        "queries/spaces/import_transaction.sql",
        "purchase",
        date,
        date,
        "",
        "Market",
        "SEK",
        -1500_i64,
        9000_i64,
        1_i64,
        "original",
        "Groceries",
        "checking",
        false
    )
    .execute(&source_db)
    .await
    .unwrap();
    let server = tokio::spawn(source.clone().listen(
        listener,
        source_db.clone(),
        source_store.clone(),
    ));
    let invite = source
        .clone()
        .invite(
            source_db.clone(),
            source_store.clone(),
            space.clone(),
            "source".into(),
        )
        .await
        .unwrap();
    let mut invalid = parse_invitation(&invite.code).unwrap();
    invalid.token[0] ^= 1;
    let invalid = format!(
        "{INVITATION_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(encode(&invalid).unwrap())
    );
    assert!(
        target
            .join(
                &target_db,
                &target_store,
                "target",
                &invalid,
                Some("127.0.0.1".into())
            )
            .await
            .is_err()
    );
    assert!(
        target_store
            .with_store(|store| store.paired_devices())
            .await
            .unwrap()
            .is_empty()
    );
    let joined = target
        .join(
            &target_db,
            &target_store,
            "target",
            &invite.code,
            Some("127.0.0.1".into()),
        )
        .await
        .unwrap();
    assert_eq!(joined, space);
    assert_eq!(note(&target_db, &space).await, "original");
    let peers = source_store
        .with_store(|store| store.paired_devices())
        .await
        .unwrap();
    assert_eq!(peers.len(), 1);
    let mut wrong_scope = target_store
        .with_store(|store| store.paired_devices())
        .await
        .unwrap()
        .remove(0);
    wrong_scope.space_id = "another-space".into();
    assert!(
        target
            .dial(
                &target_db,
                &target_store,
                &wrong_scope,
                wrong_scope.address.clone().unwrap(),
            )
            .await
            .is_err()
    );
    assert!(
        target
            .join(
                &target_db,
                &target_store,
                "target",
                &invite.code,
                Some("127.0.0.1".into())
            )
            .await
            .is_err()
    );

    let patch = serde_json::from_value(serde_json::json!({"note": "edited offline"})).unwrap();
    transactions::update_in_preview(&source_db, &source_store, &space, "purchase", patch)
        .await
        .unwrap();
    let new_account = Account {
        id: "savings".into(),
        name: "Savings".into(),
        currency: "SEK".into(),
        account_type: "savings".into(),
        account_source: "bank".into(),
        color: "#334455".into(),
        space_id: space.clone(),
    };
    let imported = ExportTransaction {
        id: "new-bank-transaction".into(),
        booking_date: date.into(),
        value_date: date.into(),
        reference: "ref".into(),
        text: "New bank purchase".into(),
        currency: "SEK".into(),
        amount: -500,
        balance: 8500,
        approved: 0,
        note: String::new(),
        category: "Other".into(),
        account_id: new_account.id.clone(),
    };
    let creation = sync_import::encode(new_account, vec![imported]).unwrap();
    source_store
        .stage_batch(space.clone(), creation.clone())
        .await
        .unwrap();
    source_store
        .stage_batch(space.clone(), creation)
        .await
        .unwrap();
    let automatic = tokio::spawn(
        target
            .clone()
            .sync_loop(target_db.clone(), target_store.clone()),
    );
    tokio::time::timeout(Duration::from_secs(15), async {
        while note(&target_db, &space).await != "edited offline" {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let imported = sqlx::query_file!(
        "queries/transactions/get_transaction_edit_fields.sql",
        r#"["new-bank-transaction"]"#,
        space
    )
    .fetch_all(&target_db)
    .await
    .unwrap();
    assert_eq!(imported.len(), 1);
    let account = sqlx::query_file_as!(Account, "queries/accounts/get_account.sql", "savings")
        .fetch_one(&target_db)
        .await
        .unwrap();
    assert_eq!(account.space_id, space);
    let reply =
        serde_json::from_value(serde_json::json!({"note": "edited on the receiving device"}))
            .unwrap();
    transactions::update_in_preview(
        &target_db,
        &target_store,
        &space,
        "new-bank-transaction",
        reply,
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let received = sqlx::query_file!(
                "queries/transactions/get_transaction_edit_fields.sql",
                r#"["new-bank-transaction"]"#,
                space
            )
            .fetch_one(&source_db)
            .await
            .unwrap();
            if received.note == "edited on the receiving device" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    automatic.abort();
    server.abort();
    source_db.close().await;
    target_db.close().await;
}

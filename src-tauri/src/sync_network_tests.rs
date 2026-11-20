use super::*;
use crate::{
    commands::{accounts::Account, spaces::ExportTransaction, transactions},
    db, sync_import,
};

async fn profile(
    directory: &Path,
    user: Option<&str>,
) -> (DbPool, SyncPreview, Arc<Network>, TcpListener) {
    let store = SyncPreview::open(directory.to_owned()).await.unwrap();
    let db = db::init(&directory.join("finance.sqlite"), &store)
        .await
        .unwrap();
    if let Some(user) = user {
        let date = "2026-01-01";
        let name = "Personal profile";
        sqlx::query_file!("queries/users/create_user.sql", user, name, date, date)
            .execute(&db)
            .await
            .unwrap();
    }
    let (network, listener) = Network::open(directory, 0).await.unwrap();
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
    let user_id = "d20be3bc-ecad-4742-a399-35a9850a8afa";
    let (source_db, source_store, source, listener) =
        profile(source_dir.path(), Some(user_id)).await;
    let (target_db, target_store, target, _unused_listener) =
        profile(target_dir.path(), None).await;
    let space = spaces::create_space_for_user(user_id, "Household", &source_db)
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
            user_id.into(),
            true,
        )
        .await
        .unwrap();
    let address = format!("127.0.0.1:{}", invite.port);
    let new_user = || {
        JoinUser::New(
            serde_json::from_value(serde_json::json!({"pin": "1234"})).unwrap(),
            None,
        )
    };
    let nearby_address = format!("mdns:{}", invite.id);
    for (address, approved) in [(&address, false), (&nearby_address, true)] {
        let candidates = if approved {
            discovery::discover(&invite.id).unwrap()
        } else {
            Vec::new()
        };
        let pairing = target.begin_join(address, candidates).await.unwrap();
        let request = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(incoming) = source.incoming.lock().unwrap().as_ref() {
                    break incoming.request.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(pairing.same_user);
        assert_eq!(request.code.len(), 6);
        assert!(
            serde_json::to_value(&pairing)
                .unwrap()
                .get("code")
                .is_none()
        );
        assert!(
            target
                .join(&target_db, &target_store, new_user(), &pairing.id)
                .await
                .is_err()
        );
        let wrong_code = if request.code == "000000" {
            "000001"
        } else {
            "000000"
        };
        assert!(target.verify_join_code(&pairing.id, wrong_code).is_err());
        target.verify_join_code(&pairing.id, &request.code).unwrap();
        assert!(
            source_store
                .with_store(|store| store.paired_devices())
                .await
                .unwrap()
                .is_empty()
        );
        source
            .respond_pairing(user_id, &request.id, approved)
            .unwrap();
        let joined = target
            .join(&target_db, &target_store, new_user(), &pairing.id)
            .await;
        if approved {
            assert_eq!(joined.unwrap(), (space.clone(), Some(user_id.to_owned())));
            assert!(
                target
                    .join(&target_db, &target_store, new_user(), &pairing.id)
                    .await
                    .is_err()
            );
        } else {
            assert!(joined.is_err());
            assert!(
                sqlx::query_file_as!(User, "queries/users/get_user.sql", user_id)
                    .fetch_optional(&target_db)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                target_store
                    .with_store(|store| store.paired_devices())
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
    }
    let user = sqlx::query_file_as!(User, "queries/users/get_user.sql", user_id)
        .fetch_one(&target_db)
        .await
        .unwrap();
    assert_eq!(user.id, user_id);
    assert_eq!(user.name, "Personal profile");
    assert!(user.has_pin);
    let local_pin = sqlx::query_file!("queries/users/verify_pin.sql", user_id)
        .fetch_one(&target_db)
        .await
        .unwrap()
        .pin_hash
        .unwrap();
    assert!(argon2::verify_encoded(&local_pin, b"1234").unwrap());
    let source_user = sqlx::query_file_as!(User, "queries/users/get_user.sql", user_id)
        .fetch_one(&source_db)
        .await
        .unwrap();
    assert!(!source_user.has_pin);
    assert_eq!(note(&target_db, &space).await, "original");
    let peers = source_store
        .with_store(|store| store.paired_devices())
        .await
        .unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(
        peers[0].display_name.as_deref(),
        Some(target.label.as_str())
    );
    let mut wrong_scope = target_store
        .with_store(|store| store.paired_devices())
        .await
        .unwrap()
        .remove(0);
    assert_eq!(
        wrong_scope.display_name.as_deref(),
        Some(source.label.as_str())
    );
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
    let before = target_store.revision(&space).unwrap();
    let automatic = tokio::spawn(
        target
            .clone()
            .sync_loop(target_db.clone(), target_store.clone()),
    );
    tokio::time::timeout(Duration::from_secs(15), async {
        while target_store.revision(&space).unwrap() == before {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(note(&target_db, &space).await, "edited offline");
    assert_eq!(target_store.revision("unrelated-space").unwrap(), 0);
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

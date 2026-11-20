use super::*;
use crate::{
    commands::{accounts::Account, spaces::ExportTransaction, transactions},
    db, sync_import,
};
use sha2::Digest;

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
    let (target_db, target_store, target, target_listener) = profile(target_dir.path(), None).await;
    let mut create = source_db.begin().await.unwrap();
    let space = spaces::create_space_for_user(user_id, "Household", &mut create, &source_store)
        .await
        .unwrap();
    create.commit().await.unwrap();
    let history = plinth_sync::consensus::MembershipLog::open(
        source_dir.path().join("sync.sqlite"),
        space.as_str(),
    )
    .unwrap()
    .membership()
    .await
    .unwrap()
    .unwrap();
    assert_eq!(history.owner_account(), user_id);
    let identity = open_identity(source.directory.clone()).await.unwrap();
    let fingerprint = sha2::Sha256::digest(identity.certificate()).into();
    assert_eq!(
        history.device(fingerprint).unwrap().role,
        plinth_sync::membership::Role::Owner
    );
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
    for (address, should_join) in [(&address, false), (&nearby_address, true)] {
        let candidates = if should_join {
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
        let code = request.code.as_deref().unwrap();
        assert_eq!(code.len(), 6);
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
        let wrong_code = if code == "000000" { "000001" } else { "000000" };
        assert!(
            target
                .verify_join_code(&pairing.id, wrong_code)
                .await
                .is_err()
        );
        assert!(
            source
                .incoming
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .request
                .code
                .is_some()
        );
        assert!(
            source_store
                .with_store(|store| store.paired_devices())
                .await
                .unwrap()
                .is_empty()
        );
        if should_join {
            target.verify_join_code(&pairing.id, code).await.unwrap();
            assert!(
                source
                    .incoming
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .request
                    .code
                    .is_none()
            );
            // Closing the host dialog must let the other device finish setting up its local PIN.
            source.cancel_invitation(&invite.id, user_id).unwrap();
            let joined = target
                .join(&target_db, &target_store, new_user(), &pairing.id)
                .await;
            assert_eq!(joined.unwrap(), (space.clone(), Some(user_id.to_owned())));
            assert!(
                target
                    .join(&target_db, &target_store, new_user(), &pairing.id)
                    .await
                    .is_err()
            );
        } else {
            target.cancel_join(&pairing.id).await;
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
    let source_history = source_store
        .membership_log(space.clone())
        .await
        .unwrap()
        .membership()
        .await
        .unwrap()
        .unwrap();
    let target_history = target_store
        .membership_log(space.clone())
        .await
        .unwrap()
        .membership()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(target_history.owner_account(), user_id);
    assert_eq!(
        source_history.device(source.device_id),
        target_history.device(source.device_id)
    );
    let joined = target_history.device(target.device_id).unwrap();
    assert_eq!(source_history.device(target.device_id), Some(joined));
    assert_eq!(joined.role, Role::Owner);
    let joined_contact = target_history.peer_contact(target.device_id).unwrap();
    assert_eq!(joined_contact.address_hint.unwrap().port(), target.port);
    assert_eq!(
        source_history.peer_contact(target.device_id),
        Some(joined_contact)
    );
    assert_eq!(
        target_history.peer_contact(source.device_id),
        source_history.peer_contact(source.device_id)
    );
    assert_eq!(target_history.peer_contacts().count(), 2);
    assert!(
        target
            .clone()
            .invite(
                target_db.clone(),
                target_store.clone(),
                space.clone(),
                user_id.into(),
                true,
            )
            .await
            .is_err()
    );
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
                None,
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
    let roster_space = space.clone();
    let source_enrollment = target_history.device(source.device_id).unwrap().enrollment;
    let progress = target_store
        .with_store(move |store| store.peer_progress(&roster_space, source_enrollment))
        .await
        .unwrap()
        .unwrap();
    assert!(!progress.heads.is_empty());
    let original_replica = source
        .permission_replica(&source_store, &space)
        .await
        .unwrap();
    let third_dir = tempfile::tempdir().unwrap();
    let (third_db, third_store, third, third_listener) = profile(third_dir.path(), None).await;
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
    let pairing = third
        .begin_join(&format!("127.0.0.1:{}", invite.port), Vec::new())
        .await
        .unwrap();
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
    third
        .verify_join_code(&pairing.id, request.code.as_deref().unwrap())
        .await
        .unwrap();
    third
        .join(&third_db, &third_store, new_user(), &pairing.id)
        .await
        .unwrap();
    let reused_replica = source
        .permission_replica(&source_store, &space)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&original_replica, &reused_replica));
    drop(original_replica);

    let third_id = third.device_id;
    let peer_on_target = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let peers = target_store
                .with_store(|store| store.paired_devices())
                .await
                .unwrap();
            if let Some(peer) = peers
                .into_iter()
                .find(|peer| certificate_fingerprint(&peer.certificate) == third_id)
            {
                break peer;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();

    let target_server = tokio::spawn(target.clone().listen(
        target_listener,
        target_db.clone(),
        target_store.clone(),
    ));
    let third_server = tokio::spawn(third.clone().listen(
        third_listener,
        third_db.clone(),
        third_store.clone(),
    ));
    // Exercise the desktop permission listener with real Raft traffic. Automatic voter
    // promotion is a separate operation from installing the desktop runtime.
    reused_replica.promote_owners().await.unwrap();
    drop(reused_replica);
    for (store, other) in [
        (&target_store, third.device_id),
        (&third_store, target.device_id),
    ] {
        let log = store.membership_log(space.clone()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !log.is_committed_voter(other).await.unwrap() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
    }
    automatic.abort();
    let _ = automatic.await;
    server.abort();
    let _ = server.await;
    source.shutdown_permissions().await.unwrap();
    assert!(
        source
            .permission_replica(&source_store, &space)
            .await
            .is_err()
    );
    let patch =
        serde_json::from_value(serde_json::json!({"note": "edited with the creator offline"}))
            .unwrap();
    transactions::update_in_preview(&target_db, &target_store, &space, "purchase", patch)
        .await
        .unwrap();
    let target_peer = third_store
        .with_store(|store| store.paired_devices())
        .await
        .unwrap()
        .into_iter()
        .find(|peer| certificate_fingerprint(&peer.certificate) == target.device_id)
        .unwrap();
    let address = target_peer.address.clone().unwrap();
    third
        .dial(&third_db, &third_store, &target_peer, address, None)
        .await
        .unwrap();
    assert_eq!(
        note(&third_db, &space).await,
        "edited with the creator offline"
    );

    let local_space = space.clone();
    target_store
        .with_store(move |store| store.unpair_device(&local_space, &peer_on_target.certificate))
        .await
        .unwrap();
    let roster = target_store
        .membership_log(space.clone())
        .await
        .unwrap()
        .membership()
        .await
        .unwrap()
        .unwrap();
    target
        .cache_roster(&target_store, &space, &roster)
        .await
        .unwrap();
    assert!(
        target_store
            .with_store(|store| store.paired_devices())
            .await
            .unwrap()
            .iter()
            .all(|peer| certificate_fingerprint(&peer.certificate) != third_id)
    );
    target_server.abort();
    third_server.abort();
    target.shutdown_permissions().await.unwrap();
    third.shutdown_permissions().await.unwrap();
    third_db.close().await;
    source_db.close().await;
    target_db.close().await;
}

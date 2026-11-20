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
    let source_log = source_store.membership_log(space.clone()).await.unwrap();
    assert!(
        !source_log
            .should_resume_replica(source.device_id)
            .await
            .unwrap()
    );
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
    drop(reused_replica);
    assert!(
        source_log
            .should_resume_replica(source.device_id)
            .await
            .unwrap()
    );
    let source_sync = tokio::spawn(
        source
            .clone()
            .sync_loop(source_db.clone(), source_store.clone()),
    );

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
    for (store, other) in [
        (&target_store, third.device_id),
        (&third_store, target.device_id),
    ] {
        let log = store.membership_log(space.clone()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(20), async {
            while !log.has_committed_shared_voters().await.unwrap()
                || !log.is_committed_voter(other).await.unwrap()
            {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
    }
    source_sync.abort();
    let _ = source_sync.await;
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

    target_server.abort();
    third_server.abort();
    let _ = target_server.await;
    let _ = third_server.await;
    target.shutdown_permissions().await.unwrap();
    third.shutdown_permissions().await.unwrap();
    let target_port = target.port;
    let third_port = third.port;
    let (target, target_listener) = Network::open(target_dir.path(), target_port).await.unwrap();
    let (third, third_listener) = Network::open(third_dir.path(), third_port).await.unwrap();
    assert_eq!(target.port, target_port);
    assert_eq!(third.port, third_port);

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
    let target_worker = tokio::spawn(
        target
            .clone()
            .sync_loop(target_db.clone(), target_store.clone()),
    );
    let third_worker = tokio::spawn(
        third
            .clone()
            .sync_loop(third_db.clone(), third_store.clone()),
    );
    let (target_replica, third_replica) = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let target_replica = target
                .permission_replicas
                .lock()
                .await
                .as_ref()
                .and_then(|replicas| replicas.get(&space))
                .cloned();
            let third_replica = third
                .permission_replicas
                .lock()
                .await
                .as_ref()
                .and_then(|replicas| replicas.get(&space))
                .cloned();
            if let (Some(target), Some(third)) = (target_replica, third_replica) {
                break (target, third);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let removal = plinth_sync::membership::Change::Remove {
        enrollment: source_enrollment,
    };
    let target_is_leader = tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            for (is_target, replica) in [(true, &target_replica), (false, &third_replica)] {
                if matches!(
                    replica.propose(removal.clone()).await,
                    Ok(plinth_sync::membership::Outcome::Removed(_)
                        | plinth_sync::membership::Outcome::EnrollmentInactive)
                ) {
                    return is_target;
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    let (invite_host, invite_db, invite_store) = if target_is_leader {
        (&third, &third_db, &third_store)
    } else {
        (&target, &target_db, &target_store)
    };
    let fourth_dir = tempfile::tempdir().unwrap();
    let (fourth_db, fourth_store, fourth, _fourth_listener) =
        profile(fourth_dir.path(), None).await;
    let forwarded_invite = invite_host
        .clone()
        .invite(
            invite_db.clone(),
            invite_store.clone(),
            space.clone(),
            user_id.into(),
            true,
        )
        .await
        .unwrap();
    let forwarded = fourth
        .begin_join(&format!("127.0.0.1:{}", forwarded_invite.port), Vec::new())
        .await
        .unwrap();
    let forwarded_code = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(code) = invite_host
                .incoming
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|pending| pending.request.code.clone())
            {
                break code;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    fourth
        .verify_join_code(&forwarded.id, &forwarded_code)
        .await
        .unwrap();
    fourth
        .join(&fourth_db, &fourth_store, new_user(), &forwarded.id)
        .await
        .unwrap();
    let forwarded_history = fourth_store
        .membership_log(space.clone())
        .await
        .unwrap()
        .membership()
        .await
        .unwrap()
        .unwrap();
    assert!(forwarded_history.permits_sync(fourth.device_id));
    assert!(!forwarded_history.permits_sync(source.device_id));
    assert_eq!(
        invite_store
            .membership_log(space.clone())
            .await
            .unwrap()
            .membership()
            .await
            .unwrap()
            .unwrap()
            .device(fourth.device_id),
        forwarded_history.device(fourth.device_id)
    );
    let leader_store = if target_is_leader {
        &target_store
    } else {
        &third_store
    };
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let devices = leader_store
                .with_store(|store| store.paired_devices())
                .await
                .unwrap();
            if devices
                .iter()
                .any(|peer| certificate_fingerprint(&peer.certificate) == fourth.device_id)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the permission leader learns the new device through the shared roster");
    let leader = if target_is_leader { &target } else { &third };
    let leader_id = space_devices(invite_store, &space)
        .await
        .unwrap()
        .into_iter()
        .find(|peer| certificate_fingerprint(&peer.certificate) == leader.device_id)
        .map(|peer| certificate_id(&peer.certificate))
        .unwrap();
    assert!(matches!(
        invite_host.remove_device(invite_store, &space, user_id, &leader_id).await,
        Err(AppError::InvalidInput(message)) if message.contains("too few voting devices")
    ));
    let fourth_peer = space_devices(invite_store, &space)
        .await
        .unwrap()
        .into_iter()
        .find(|peer| certificate_fingerprint(&peer.certificate) == fourth.device_id)
        .unwrap();
    let fourth_id = certificate_id(&fourth_peer.certificate);
    invite_host
        .remove_device(invite_store, &space, user_id, &fourth_id)
        .await
        .unwrap();
    invite_host
        .remove_device(invite_store, &space, user_id, &fourth_id)
        .await
        .unwrap();
    let enrollment = forwarded_history
        .device(fourth.device_id)
        .unwrap()
        .enrollment;
    for store in [invite_store, leader_store] {
        let roster = store
            .membership_log(space.clone())
            .await
            .unwrap()
            .membership()
            .await
            .unwrap()
            .unwrap();
        assert!(roster.removal_index(enrollment).is_some());
        assert!(roster.permits_sync(leader.device_id));
        assert!(
            space_devices(store, &space)
                .await
                .unwrap()
                .iter()
                .all(|peer| peer.certificate != fourth_peer.certificate)
        );
    }
    assert!(matches!(
        invite_host
            .current_peer(invite_db, invite_store, &space, &fourth_peer.certificate)
            .await,
        Err(AppError::Forbidden)
    ));
    let fourth_log = fourth_store.membership_log(space.clone()).await.unwrap();
    let fourth_peers = space_devices(&fourth_store, &space).await.unwrap();
    let inviter_peer = fourth_peers
        .iter()
        .find(|peer| certificate_fingerprint(&peer.certificate) == invite_host.device_id)
        .unwrap();
    let first_report = invite_store
        .membership_log(space.clone())
        .await
        .unwrap()
        .withdrawal_for(invite_host.device_id, enrollment)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !fourth_log
            .record_withdrawal(fourth.device_id, source.device_id, first_report.clone())
            .await
            .unwrap()
    );
    let inviter_address = format!("127.0.0.1:{}", invite_host.port);
    for _ in 0..2 {
        assert!(
            !fourth
                .request_withdrawal(&fourth_store, inviter_peer, &inviter_address)
                .await
                .unwrap()
        );
    }
    assert!(
        fourth_log
            .received_withdrawal(fourth.device_id)
            .await
            .unwrap()
            .is_none()
    );
    let leader_peer = fourth_peers
        .into_iter()
        .find(|peer| certificate_fingerprint(&peer.certificate) == leader.device_id)
        .unwrap();
    fourth
        .sync_peer(&fourth_db, &fourth_store, leader_peer.clone())
        .await;
    drop(fourth_log);
    let saved = fourth_store
        .membership_log(space.clone())
        .await
        .unwrap()
        .received_withdrawal(fourth.device_id)
        .await
        .unwrap();
    assert_eq!(saved, Some(first_report));
    assert!(matches!(
        fourth
            .current_peer(&fourth_db, &fourth_store, &space, &leader_peer.certificate)
            .await,
        Err(AppError::Forbidden)
    ));
    assert_eq!(
        note(&fourth_db, &space).await,
        note(invite_db, &space).await
    );
    fourth_db.close().await;
    drop(target_replica);
    drop(third_replica);
    target_worker.abort();
    third_worker.abort();
    let _ = target_worker.await;
    let _ = third_worker.await;

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

    let leader_db = if target_is_leader {
        &target_db
    } else {
        &third_db
    };
    let leader_log = leader_store.membership_log(space.clone()).await.unwrap();
    let partition = invite_host.permission_calls.write().await;
    assert!(matches!(
        leader.delete_owned_space(leader_db, leader_store, &space, user_id).await,
        Err(AppError::Io(message)) if message.contains("could not confirm the change")
    ));
    assert!(
        leader_log
            .membership()
            .await
            .unwrap()
            .unwrap()
            .deletion_index()
            .is_none()
    );
    assert_eq!(
        note(leader_db, &space).await,
        "edited with the creator offline"
    );
    drop(partition);

    invite_host
        .delete_owned_space(invite_db, invite_store, &space, user_id)
        .await
        .unwrap();
    let deletion_index = invite_store
        .membership_log(space.clone())
        .await
        .unwrap()
        .membership()
        .await
        .unwrap()
        .unwrap()
        .deletion_index();
    assert!(deletion_index.is_some());
    assert!(
        sqlx::query_file!("queries/spaces/get_space_name.sql", space)
            .fetch_optional(invite_db)
            .await
            .unwrap()
            .is_none()
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while leader_log
            .membership()
            .await
            .unwrap()
            .unwrap()
            .deletion_index()
            != deletion_index
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        note(leader_db, &space).await,
        "edited with the creator offline"
    );
    leader
        .delete_owned_space(leader_db, leader_store, &space, user_id)
        .await
        .unwrap();
    assert!(
        sqlx::query_file!("queries/spaces/get_space_name.sql", space)
            .fetch_optional(leader_db)
            .await
            .unwrap()
            .is_none()
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

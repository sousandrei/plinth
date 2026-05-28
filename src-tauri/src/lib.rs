#![allow(dead_code)]

mod classifier;
mod commands;
mod db;
mod error;
mod import;

pub use error::AppError;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

pub type ClassifierState = Arc<Mutex<Option<classifier::Classifier>>>;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let handle2 = handle.clone();
            tauri::async_runtime::block_on(async move { db::setup(&handle).await })?;

            // Classifier starts as None; a background task loads it and emits
            // classifier://ready (or classifier://error) when done.
            let classifier_state: ClassifierState = Arc::new(Mutex::new(None));
            app.manage(classifier_state.clone());

            tauri::async_runtime::spawn(async move {
                let resource_dir = match handle2.path().resource_dir() {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = handle2.emit("classifier://error", e.to_string());
                        return;
                    }
                };
                let app_data_dir = match handle2.path().app_data_dir() {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = handle2.emit("classifier://error", e.to_string());
                        return;
                    }
                };

                let db = handle2.state::<db::DbPool>();
                let classes = match sqlx::query_file!("queries/categories/list_all_categories.sql")
                    .fetch_all(&*db)
                    .await
                {
                    Ok(rows) => rows.into_iter().map(|r| r.name).collect::<Vec<String>>(),
                    Err(e) => {
                        let _ =
                            handle2.emit("classifier://error", format!("fetch categories: {e}"));
                        return;
                    }
                };

                match classifier::Classifier::load(&resource_dir, &app_data_dir, classes) {
                    Ok(cl) => {
                        if let Ok(mut guard) = classifier_state.lock() {
                            *guard = Some(cl);
                        }
                        let _ = handle2.emit("classifier://ready", ());
                    }
                    Err(e) => {
                        let _ = handle2.emit("classifier://error", e.to_string());
                    }
                }
            });

            app.manage(commands::training::TrainingHistory::default());
            app.manage(commands::training::CancelToken::default());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::users::list_users,
            commands::users::create_user,
            commands::users::set_pin,
            commands::users::verify_pin,
            commands::users::update_user_name,
            commands::users::factory_reset,
            commands::accounts::list_accounts,
            commands::accounts::update_account,
            commands::transactions::list_transactions,
            commands::transactions::update_transaction,
            commands::categories::list_all_categories,
            commands::categories::create_category,
            commands::categories::delete_category,
            commands::categories::update_category,
            commands::upload::list_parsers,
            commands::upload::upload_file,
            commands::upload::list_parser_files,
            commands::upload::save_parser_file,
            commands::upload::test_parser_transform,
            commands::upload::classify_transactions,
            commands::aggregations::get_aggregations,
            commands::account_summaries::list_account_summaries,
            commands::account_summaries::upsert_account_summary,
            commands::account_summaries::delete_account_summary,
            commands::training::fine_tune,
            commands::training::list_models,
            commands::training::set_active_model,
            commands::training::get_training_samples,
            commands::training::get_training_progress,
            commands::training::count_approved_transactions,
            commands::training::stop_training,
            commands::training::delete_model,
            commands::training::is_classifier_ready,
            commands::training::get_training_device,
            commands::minilm::minilm_status,
            commands::minilm::ensure_minilm,
            commands::settings::get_app_setting,
            commands::settings::set_app_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

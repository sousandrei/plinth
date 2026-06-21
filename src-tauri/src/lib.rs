mod classifier;
mod commands;
mod db;
mod error;
mod import;
mod sync;

pub use error::AppError;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

pub type ClassifierState = Arc<Mutex<Option<classifier::Classifier>>>;

// ---------------------------------------------------------------------------
// Session state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SessionData {
    pub user_id: String,
    pub space_id: Option<String>,
}

/// Fully resolved session — both user and active space are present.
#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub user_id: String,
    pub space_id: String,
}

pub struct Session(Mutex<Option<SessionData>>);

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Option<SessionData>>, AppError> {
        self.0
            .lock()
            .map_err(|e| AppError::Internal(format!("session mutex poisoned: {e}")))
    }

    pub fn set(&self, user_id: String, space_id: Option<String>) -> Result<(), AppError> {
        let mut guard = self.lock()?;
        *guard = Some(SessionData { user_id, space_id });
        Ok(())
    }

    pub fn set_space(&self, space_id: String) -> Result<(), AppError> {
        let mut guard = self.lock()?;
        if let Some(ref mut data) = *guard {
            data.space_id = Some(space_id);
        }
        Ok(())
    }

    pub fn clear_space(&self) -> Result<(), AppError> {
        let mut guard = self.lock()?;
        if let Some(ref mut data) = *guard {
            data.space_id = None;
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<(), AppError> {
        let mut guard = self.lock()?;
        *guard = None;
        Ok(())
    }

    /// Returns a fully resolved session (user + active space).
    /// Fails with Unauthorized if not logged in, InvalidInput if no space selected.
    pub fn require(&self) -> Result<ActiveSession, AppError> {
        let guard = self.lock()?;
        match &*guard {
            None => Err(AppError::Unauthorized),
            Some(d) => match &d.space_id {
                None => Err(AppError::InvalidInput("no active space selected".into())),
                Some(space_id) => Ok(ActiveSession {
                    user_id: d.user_id.clone(),
                    space_id: space_id.clone(),
                }),
            },
        }
    }

    /// Returns session data even if no space is selected yet.
    /// Use only for commands that don't need a space (e.g. list_my_spaces).
    pub fn require_user(&self) -> Result<SessionData, AppError> {
        let guard = self.lock()?;
        guard.clone().ok_or(AppError::Unauthorized)
    }
}

// ---------------------------------------------------------------------------
// Tauri app entry
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let handle2 = handle.clone();
            tauri::async_runtime::block_on(async move { db::setup(&handle).await })?;

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
            app.manage(Session::new());

            let db = app.state::<db::DbPool>().inner().clone();
            let app_handle = app.handle().clone();
            let runtime = tauri::async_runtime::block_on(async move {
                sync::startup::start(app_handle, db).await
            })?;
            app.manage(runtime.peers.clone());
            app.manage(runtime.pairing.clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::users::add_app_user,
            commands::users::list_users,
            commands::users::create_user,
            commands::users::create_user_in_space,
            commands::users::set_pin,
            commands::users::verify_pin,
            commands::users::update_user_name,
            commands::users::remove_user,
            commands::users::factory_reset,
            commands::spaces::list_my_spaces,
            commands::spaces::create_space,
            commands::spaces::set_active_space,
            commands::spaces::logout,
            commands::spaces::list_space_members,
            commands::spaces::add_space_member,
            commands::spaces::remove_space_member,
            commands::spaces::leave_space,
            commands::spaces::delete_space,
            commands::spaces::evict_space,
            commands::spaces::rename_space,
            commands::spaces::update_member_role,
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
            commands::training::reload_classifier,
            commands::minilm::minilm_status,
            commands::minilm::ensure_minilm,
            commands::settings::get_app_setting,
            commands::settings::set_app_setting,
            commands::sync::list_peers,
            commands::sync::list_trusted_devices,
            commands::sync::remove_trusted_device,
            commands::sync::generate_pair_token,
            commands::sync::accept_pair_token,
            commands::sync::accept_pair_token_from_peer,
            commands::sync::join_space,
            commands::sync::get_device_name,
            commands::sync::get_local_address,
            commands::spaces::export_space_data,
            commands::spaces::import_space_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

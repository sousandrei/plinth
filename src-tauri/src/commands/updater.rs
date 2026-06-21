use tauri::AppHandle;

use crate::AppError;

#[tauri::command]
pub async fn restart_app(app: AppHandle) -> Result<bool, AppError> {
    app.restart();
}

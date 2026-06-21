use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::db::DbPool;
use crate::import::{
    engine::{ParseResult, run_transform},
    extract::extract,
    registry::{ParserUnit, find, scan},
};
use crate::{AppError, Session};

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct UploadResult {
    pub inserted: i64,
    pub skipped: i64,
    pub account_id: String,
    pub logs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ParserUnitInfo {
    pub key: String,
    pub name: String,
    pub bank: String,
    pub format: String,
    pub account_type: String,
    pub account_source: String,
    pub is_builtin: bool,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parser_dirs(app: &AppHandle) -> Result<(PathBuf, PathBuf), AppError> {
    let builtin = app
        .path()
        .resource_dir()
        .map_err(|e| AppError::Io(format!("resource_dir: {e}")))?
        .join("parsers");

    let user = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Io(format!("app_data_dir: {e}")))?
        .join("parsers");

    std::fs::create_dir_all(&user)
        .map_err(|e| AppError::Io(format!("create parsers dir {}: {e}", user.display())))?;

    Ok((builtin, user))
}

fn get_units(app: &AppHandle) -> Result<Vec<ParserUnit>, AppError> {
    let (builtin, user) = parser_dirs(app)?;
    Ok(scan(&builtin, &user))
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_parsers(app: AppHandle) -> Result<Vec<ParserUnitInfo>, AppError> {
    let units = tokio::task::spawn_blocking(move || get_units(&app))
        .await
        .map_err(|e| AppError::Internal(format!("list_parsers spawn: {e}")))??;

    Ok(units
        .into_iter()
        .map(|u| ParserUnitInfo {
            key: u.key,
            name: u.name,
            bank: u.bank,
            format: u.format,
            account_type: u.account_type,
            account_source: u.account_source,
            is_builtin: u.is_builtin,
        })
        .collect())
}

#[tauri::command]
pub async fn upload_file(
    file_path: String,
    parser_key: String,
    app: AppHandle,
    session: State<'_, Session>,
    db: State<'_, DbPool>,
    classifier: State<'_, crate::ClassifierState>,
) -> Result<UploadResult, AppError> {
    let space_id = session.require()?.space_id;
    let path = PathBuf::from(&file_path);
    let mut logs = Vec::new();

    logs.push(format!("Resolving parser format: {parser_key}"));

    // Resolve the parser unit and run extraction + transform in a blocking thread.
    let (unit_key, unit_account_source, unit_currency, script_path) = {
        let units = get_units(&app)?;
        let unit = find(&units, &parser_key)?;
        (
            unit.key.clone(),
            unit.account_source.clone(),
            unit.currency.clone(),
            unit.script_path.clone(),
        )
    };

    logs.push(format!("Using parser script: {}", script_path.display()));
    logs.push(format!(
        "Target account properties — Bank: {unit_account_source}, Currency: {unit_currency}"
    ));
    logs.push(format!(
        "Extracting contents from document: {}",
        path.display()
    ));

    let (parse_result, script_logs) = tokio::task::spawn_blocking(move || {
        let content = extract(&path)?;
        run_transform(&script_path, &unit_key, content)
    })
    .await
    .map_err(|e| AppError::Internal(format!("upload spawn: {e}")))??;

    logs.push("Successfully executed JS transform engine.".to_string());
    for slog in script_logs {
        logs.push(format!("  [SCRIPT] {slog}"));
    }

    let mut tx = db
        .inner()
        .begin()
        .await
        .map_err(|e| AppError::Db(format!("upload begin: {e}")))?;

    let account_id: String;
    let mut inserted = 0i64;
    let mut skipped = 0i64;

    match parse_result {
        ParseResult::Checking {
            account_id: ref aid,
            ref transactions,
        }
        | ParseResult::Savings {
            account_id: ref aid,
            ref transactions,
        } => {
            account_id = aid.clone();
            let account_type = if matches!(parse_result, ParseResult::Checking { .. }) {
                "checking"
            } else {
                "savings"
            };
            logs.push("Ensuring target account exists in database...".to_string());
            ensure_account_exists(
                &mut tx,
                &space_id,
                &account_id,
                &account_id,
                account_type,
                &unit_account_source,
                &unit_currency,
            )
            .await?;
            insert_transactions(
                &mut tx,
                &classifier,
                transactions,
                &account_id,
                &unit_currency,
                &mut ImportAccumulator {
                    inserted: &mut inserted,
                    skipped: &mut skipped,
                    logs: &mut logs,
                },
            )
            .await?;
        }

        ParseResult::Investment {
            account_id: aid,
            month,
            balance,
        } => {
            account_id = aid;

            logs.push("Ensuring target account exists in database...".to_string());
            ensure_account_exists(
                &mut tx,
                &space_id,
                &account_id,
                &account_id,
                "investment",
                &unit_account_source,
                &unit_currency,
            )
            .await?;

            logs.push(format!(
                "Upserting monthly summary — month: {month}, balance: {} {unit_currency}",
                balance as f64 / 100.0,
            ));

            sqlx::query_file!(
                "queries/account_summaries/upsert_account_summary.sql",
                month,
                account_id,
                balance
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Db(format!("upload upsert summary: {e}")))?;

            inserted = 1;
            logs.push("Account summary updated.".to_string());
        }
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Db(format!("upload commit: {e}")))?;

    Ok(UploadResult {
        inserted,
        skipped,
        account_id,
        logs,
    })
}

// ── ensure_account_exists ────────────────────────────────────────────────────

async fn ensure_account_exists(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    account_id: &str,
    name: &str,
    account_type: &str,
    account_source: &str,
    currency: &str,
) -> Result<(), AppError> {
    sqlx::query_file!(
        "queries/upload/ensure_account_exists.sql",
        account_id,
        name,
        currency,
        account_type,
        account_source,
        space_id
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Db(format!("ensure_account_exists: {e}")))?;

    Ok(())
}

struct ImportAccumulator<'a> {
    inserted: &'a mut i64,
    skipped: &'a mut i64,
    logs: &'a mut Vec<String>,
}

async fn insert_transactions(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    classifier: &crate::ClassifierState,
    transactions: &[crate::import::engine::ParsedTransaction],
    account_id: &str,
    currency: &str,
    acc: &mut ImportAccumulator<'_>,
) -> Result<(), AppError> {
    acc.logs
        .push(format!("Processing {} transactions...", transactions.len()));

    for parsed in transactions {
        let category = {
            let guard = classifier
                .lock()
                .map_err(|e| AppError::Internal(format!("failed to lock classifier: {e}")))?;
            match guard.as_ref() {
                Some(cl) => cl.predict(&parsed.text, parsed.amount, &parsed.booking_date)?,
                None => "Other".to_string(),
            }
        };

        let rows = sqlx::query_file!(
            "queries/upload/insert_transaction.sql",
            parsed.id,
            parsed.booking_date,
            parsed.value_date,
            parsed.reference,
            parsed.text,
            currency,
            parsed.amount,
            parsed.balance,
            category,
            account_id
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Db(format!("insert_transactions: {e}")))?
        .rows_affected();

        if rows > 0 {
            *acc.inserted += 1;
            acc.logs.push(format!(
                "  [INSERTED] {} — {} ({} {})",
                parsed.booking_date,
                parsed.text,
                parsed.amount as f64 / 100.0,
                currency
            ));
        } else {
            *acc.skipped += 1;
            acc.logs.push(format!(
                "  [SKIPPED] Duplicate [{}] {}",
                parsed.id, parsed.booking_date
            ));
        }
    }

    acc.logs.push(format!(
        "Finished: {} inserted, {} skipped.",
        acc.inserted, acc.skipped
    ));
    Ok(())
}

#[derive(Debug, serde::Serialize)]
pub struct ParserFileInfo {
    pub filename: String,
    pub path: String,
    pub is_builtin: bool,
    pub content: String,
    pub units: Vec<ParserUnitInfo>,
}

#[tauri::command]
pub async fn list_parser_files(app: AppHandle) -> Result<Vec<ParserFileInfo>, AppError> {
    tokio::task::spawn_blocking(move || {
        let (builtin, user) = parser_dirs(&app)?;
        let mut files = Vec::new();

        for (dir, is_builtin) in [(builtin, true), (user, false)] {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("js") {
                        continue;
                    }
                    let filename = path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or_default()
                        .to_string();

                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let units = match crate::import::engine::extract_units_source(&content) {
                            Ok(metas) => metas
                                .into_iter()
                                .map(|m| ParserUnitInfo {
                                    key: m.key,
                                    name: m.name,
                                    bank: m.bank,
                                    format: m.format,
                                    account_type: m.account_type,
                                    account_source: m.account_source,
                                    is_builtin,
                                })
                                .collect(),
                            Err(_) => Vec::new(),
                        };

                        files.push(ParserFileInfo {
                            filename,
                            path: path.to_string_lossy().to_string(),
                            is_builtin,
                            content,
                            units,
                        });
                    }
                }
            }
        }

        files.sort_by(|a, b| {
            b.is_builtin
                .cmp(&a.is_builtin)
                .then_with(|| a.filename.cmp(&b.filename))
        });

        Ok(files)
    })
    .await
    .map_err(|e| AppError::Internal(format!("list_parser_files spawn: {e}")))?
}

#[tauri::command]
pub async fn save_parser_file(path: String, code: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let filepath = PathBuf::from(&path);
        std::fs::write(&filepath, code).map_err(|e| {
            AppError::Io(format!(
                "failed to write parser file {}: {e}",
                filepath.display()
            ))
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("save_parser_file spawn: {e}")))?
}

#[derive(Debug, serde::Serialize)]
pub struct TestTransformResult {
    pub result: String,
    pub logs: Vec<String>,
}

#[tauri::command]
pub async fn test_parser_transform(
    file_path: String,
    script_code: String,
    unit_key: String,
) -> Result<TestTransformResult, AppError> {
    tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&file_path);
        let content = extract(&path)?;
        let (result, logs) =
            crate::import::engine::run_test_transform(&script_code, &unit_key, content)?;
        Ok(TestTransformResult { result, logs })
    })
    .await
    .map_err(|e| AppError::Internal(format!("test_parser_transform spawn: {e}")))?
}

#[derive(Debug, serde::Deserialize)]
pub struct TransactionInput {
    pub text: String,
    pub amount: i64,
    pub booking_date: String,
}

#[tauri::command]
pub async fn classify_transactions(
    transactions: Vec<TransactionInput>,
    classifier: tauri::State<'_, crate::ClassifierState>,
) -> Result<Vec<String>, AppError> {
    let guard = classifier
        .lock()
        .map_err(|e| AppError::Internal(format!("failed to lock classifier: {e}")))?;
    let classifier = guard
        .as_ref()
        .ok_or_else(|| AppError::Internal("classifier not ready".to_string()))?;
    let mut results = Vec::with_capacity(transactions.len());
    for tx in transactions {
        let category = classifier.predict(&tx.text, tx.amount, &tx.booking_date)?;
        results.push(category);
    }
    Ok(results)
}

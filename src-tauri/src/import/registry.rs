use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::engine::extract_units;
use crate::AppError;

#[derive(Debug, Clone, Serialize)]
pub struct ParserUnit {
    pub key: String,
    pub name: String,
    pub bank: String,
    pub format: String,
    pub account_type: String,
    pub account_source: String,
    pub currency: String,
    pub script_path: PathBuf,
    pub is_builtin: bool,
}

// Scan both directories and return all discovered parser units.
// User scripts in app_data_dir override built-ins with the same key.
pub fn scan(builtin_dir: &Path, user_dir: &Path) -> Vec<ParserUnit> {
    let mut units: HashMap<String, ParserUnit> = HashMap::new();

    for (dir, is_builtin) in [(builtin_dir, true), (user_dir, false)] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("js") {
                    continue;
                }
                match extract_units(&path) {
                    Ok(metas) => {
                        for meta in metas {
                            units.insert(
                                meta.key.clone(),
                                ParserUnit {
                                    key: meta.key,
                                    name: meta.name,
                                    bank: meta.bank,
                                    format: meta.format,
                                    account_type: meta.account_type,
                                    account_source: meta.account_source,
                                    currency: meta.currency,
                                    script_path: path.clone(),
                                    is_builtin,
                                },
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("plinth: skipping {}: {e}", path.display());
                    }
                }
            }
        }
    }

    let mut result: Vec<ParserUnit> = units.into_values().collect();
    result.sort_by(|a, b| a.bank.cmp(&b.bank).then(a.name.cmp(&b.name)));
    result
}

// Look up a single unit from the scanned list.
pub fn find<'a>(units: &'a [ParserUnit], key: &str) -> Result<&'a ParserUnit, AppError> {
    units
        .iter()
        .find(|u| u.key == key)
        .ok_or_else(|| AppError::InvalidInput(format!("unknown parser key: {key}")))
}

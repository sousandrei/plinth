use std::collections::HashMap;
use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};

use crate::AppError;

pub enum ExtractedContent {
    Rows(Vec<HashMap<String, String>>),
    Text(String),
}

pub fn extract(path: &Path) -> Result<ExtractedContent, AppError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "xlsx" | "xls" | "xlsm" | "xlsb" | "ods" => extract_xlsx(path),
        "csv" => extract_csv(path),
        "pdf" => extract_pdf(path),
        other => Err(AppError::InvalidInput(format!(
            "unsupported file extension: .{other}"
        ))),
    }
}

fn extract_xlsx(path: &Path) -> Result<ExtractedContent, AppError> {
    let mut workbook =
        open_workbook_auto(path).map_err(|e| AppError::Io(format!("extract_xlsx open: {e}")))?;

    let sheet_name = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| AppError::InvalidInput("xlsx has no sheets".into()))?;

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| AppError::Io(format!("extract_xlsx sheet: {e}")))?;

    let mut all_rows: Vec<Vec<String>> = range
        .rows()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    Data::String(s) => s.trim().to_string(),
                    Data::Float(f) => {
                        if f.fract() == 0.0 {
                            format!("{}", *f as i64)
                        } else {
                            f.to_string()
                        }
                    }
                    Data::Int(i) => i.to_string(),
                    Data::Bool(b) => b.to_string(),
                    Data::DateTime(dt) => dt.to_string(),
                    Data::Empty => String::new(),
                    _ => String::new(),
                })
                .collect()
        })
        .collect();

    rows_to_objects(&mut all_rows)
}

fn extract_csv(path: &Path) -> Result<ExtractedContent, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|e| AppError::Io(format!("extract_csv open: {e}")))?;

    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| AppError::Io(format!("extract_csv headers: {e}")))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();

    let mut objects: Vec<HashMap<String, String>> = Vec::new();

    for result in reader.records() {
        let record = result.map_err(|e| AppError::Io(format!("extract_csv record: {e}")))?;
        let mut map = HashMap::new();
        for (i, value) in record.iter().enumerate() {
            let key = headers.get(i).cloned().unwrap_or_else(|| i.to_string());
            map.insert(key, value.trim().to_string());
        }
        objects.push(map);
    }

    Ok(ExtractedContent::Rows(objects))
}

fn extract_pdf(path: &Path) -> Result<ExtractedContent, AppError> {
    let bytes = std::fs::read(path).map_err(|e| AppError::Io(format!("extract_pdf read: {e}")))?;

    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| AppError::Io(format!("extract_pdf parse: {e}")))?;

    Ok(ExtractedContent::Text(text))
}

// Turn a list of raw string rows into named-key objects using the header row (first row with at least 3 non-empty columns).
fn rows_to_objects(rows: &mut [Vec<String>]) -> Result<ExtractedContent, AppError> {
    if rows.is_empty() {
        return Ok(ExtractedContent::Rows(vec![]));
    }

    // Find the first row that has at least 3 non-empty cells — treat it as the header row.
    let header_idx = rows
        .iter()
        .position(|r| r.iter().filter(|c| !c.is_empty()).count() >= 3)
        .unwrap_or(0);

    let headers: Vec<String> = rows[header_idx].clone();

    // Map ALL rows (including metadata rows before header_idx) to retain structural metadata (e.g. account numbers in headers)
    let objects: Vec<HashMap<String, String>> = rows
        .iter()
        .filter(|r| r.iter().any(|c| !c.is_empty()))
        .map(|row| {
            let mut map = HashMap::new();
            for (i, value) in row.iter().enumerate() {
                let key = headers.get(i).cloned().unwrap_or_else(|| i.to_string());
                if !key.is_empty() {
                    map.insert(key, value.clone());
                }
            }
            map
        })
        .collect();

    Ok(ExtractedContent::Rows(objects))
}

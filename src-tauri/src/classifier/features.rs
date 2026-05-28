use chrono::{Datelike, NaiveDate};

use crate::error::AppError;

pub const FEATURE_DIM: usize = 64;

const AMOUNT_BIN_EDGES: &[f32] = &[
    -100000.0, -10000.0, -1000.0, -100.0, -10.0, 0.0, 10.0, 100.0, 1000.0, 10000.0, 100000.0,
];

const NUM_BINS: usize = 13;
const NUM_WEEKDAYS: usize = 7;
const NUM_MONTHDAYS: usize = 31;
const NUM_MONTHS: usize = 12;

pub fn build_features(amount_minor: i64, date_str: &str) -> Result<Vec<f32>, AppError> {
    let mut features = Vec::with_capacity(FEATURE_DIM);

    let amount_base = amount_minor as f32 / 100.0;

    // Binned amount (13 dimensions)
    let mut bin_idx = AMOUNT_BIN_EDGES.len();
    for (idx, &edge) in AMOUNT_BIN_EDGES.iter().enumerate() {
        if amount_base < edge {
            bin_idx = idx;
            break;
        }
    }
    for i in 0..NUM_BINS {
        features.push(if i == bin_idx { 1.0 } else { 0.0 });
    }

    // Continuous signed log-scale amount (1 dimension)
    let sign = if amount_base >= 0.0 { 1.0 } else { -1.0 };
    features.push(sign * (amount_base.abs() + 1.0).ln());

    let parsed_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&date_str[..10], "%Y-%m-%d"))
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());

    let weekday = parsed_date.weekday().num_days_from_monday() as usize;
    let monthday = parsed_date.day() as usize;
    let month = parsed_date.month() as usize;

    // One-hot weekday (7 dimensions)
    for i in 0..NUM_WEEKDAYS {
        features.push(if i == weekday { 1.0 } else { 0.0 });
    }

    // One-hot monthday (31 dimensions)
    for i in 1..=NUM_MONTHDAYS {
        features.push(if i == monthday { 1.0 } else { 0.0 });
    }

    // One-hot month (12 dimensions)
    for i in 1..=NUM_MONTHS {
        features.push(if i == month { 1.0 } else { 0.0 });
    }

    debug_assert_eq!(
        features.len(),
        FEATURE_DIM,
        "feature vector length mismatch"
    );

    Ok(features)
}

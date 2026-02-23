use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use chrono::Utc;
use std::collections::HashMap;
use sha2::{Sha256, Digest};
use colored::*;
use colored::control;

// ======================================================
// DATA STRUCTURE
// ======================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RunRecord {
    pub stack: String,
    pub timestamp: String,
    pub duration_seconds: u64,
    pub readiness: Option<u32>,
    pub confidence: u32,
    pub risk: String,
    pub exit_code: i32,
    pub services: HashMap<String, u32>,
    pub hash: Option<String>,
}

// ======================================================
// HASH
// ======================================================

fn compute_hash(record: &RunRecord) -> Result<String, String> {
    let mut temp = record.clone();
    temp.hash = None;

    let json = serde_json::to_string(&temp)
        .map_err(|e| format!("Hash serialization error: {}", e))?;

    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());

    Ok(format!("{:x}", hasher.finalize()))
}

// ======================================================
// STRICT INTEGRITY CHECK
// ======================================================

pub fn validate_stack_integrity(stack: &str) -> Result<(), String> {

    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    let stack_dir = home.join(".rehearsa").join("history").join(stack);

    if !stack_dir.exists() {
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(&stack_dir)
        .map_err(|e| format!("Failed to read stack dir: {}", e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();

    entries.sort();

    for path in entries {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;

        let record: RunRecord = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;

        if let Some(stored_hash) = &record.hash {
            let calculated = compute_hash(&record)?;
            if &calculated != stored_hash {
                return Err(format!(
                    "Integrity violation detected in {}",
                    path.display()
                ));
            }
        }
    }

    Ok(())
}

// ======================================================
// PERSIST
// ======================================================

pub fn persist(record: &RunRecord) -> Result<(), String> {

    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    let stack_dir = home.join(".rehearsa").join("history").join(&record.stack);

    fs::create_dir_all(&stack_dir)
        .map_err(|e| format!("Failed to create history directory: {}", e))?;

    let filename = format!("{}.json", record.timestamp.replace(":", "-"));
    let file_path = stack_dir.join(filename);

    let mut record_with_hash = record.clone();
    let hash = compute_hash(&record_with_hash)?;
    record_with_hash.hash = Some(hash);

    let json = serde_json::to_string_pretty(&record_with_hash)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;

    fs::write(file_path, json)
        .map_err(|e| format!("Failed to write history file: {}", e))?;

    Ok(())
}

// ======================================================
// TIMESTAMP
// ======================================================

pub fn now_timestamp() -> String {
    Utc::now().to_rfc3339()
}

// ======================================================
// LOAD LATEST
// ======================================================

pub fn load_latest(stack: &str) -> Option<RunRecord> {

    let home = dirs::home_dir()?;
    let stack_dir = home.join(".rehearsa").join("history").join(stack);

    let mut entries: Vec<PathBuf> = fs::read_dir(stack_dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();

    entries.sort();

    let latest = entries.last()?;
    let content = fs::read_to_string(latest).ok()?;

    serde_json::from_str(&content).ok()
}

// ======================================================
// STABILITY
// ======================================================

pub fn calculate_stability(stack: &str, window: usize) -> u32 {

    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return 100,
    };

    let stack_dir = home.join(".rehearsa").join("history").join(stack);

    if !stack_dir.exists() {
        return 100;
    }

    let mut entries: Vec<PathBuf> = match fs::read_dir(&stack_dir) {
        Ok(e) => e.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return 100,
    };

    entries.sort();

    let recent = entries.into_iter().rev().take(window);

    let mut total = 0;
    let mut count = 0;

    for path in recent {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(record) = serde_json::from_str::<RunRecord>(&content) {
                total += record.confidence;
                count += 1;
            }
        }
    }

    if count == 0 { 100 } else { total / count }
}

// ======================================================
// LIST STACKS
// ======================================================

pub fn list_stacks() -> Result<(), String> {

    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    let history_dir = home.join(".rehearsa").join("history");

    if !history_dir.exists() {
        println!("No history found.");
        return Ok(());
    }

    for entry in fs::read_dir(history_dir)
        .map_err(|e| format!("Failed to read history dir: {}", e))?
    {
        let entry = entry
            .map_err(|e| format!("Failed to read entry: {}", e))?;

        if entry.path().is_dir() {
            println!("{}", entry.file_name().to_string_lossy());
        }
    }

    Ok(())
}

// ======================================================
// SHOW STACK
// ======================================================

pub fn show_stack(stack: &str) -> Result<(), String> {

    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    let stack_dir = home.join(".rehearsa").join("history").join(stack);

    if !stack_dir.exists() {
        println!("No history for stack '{}'", stack);
        return Ok(());
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(&stack_dir)
        .map_err(|e| format!("Failed to read stack dir: {}", e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();

    entries.sort();

    println!("Stack: {}\n", stack);

    for path in entries {

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let record: RunRecord = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse history file: {}", e))?;

        println!(
            "{} | Readiness: {}% | Confidence: {}% | Risk: {} | Duration: {}s | Exit: {}",
            record.timestamp,
            record.readiness.unwrap_or(0),
            record.confidence,
            record.risk,
            record.duration_seconds,
            record.exit_code
        );
    }

    Ok(())
}

// ======================================================
// STATUS
// ======================================================

pub fn status_all() -> Result<(), String> {

    control::set_override(true);

    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    let history_dir = home.join(".rehearsa").join("history");

    if !history_dir.exists() {
        println!("No history found.");
        return Ok(());
    }

    println!();
    println!("{}", "Rehearsa Status".bold());
    println!("{}", "────────────────────────────────────────────────────────────────────".dimmed());
    println!();

    println!(
        "{:<20} {:<12} {:<12} {:<12} {:<12} {:<6}",
        "Stack", "Readiness", "Confidence", "Risk", "Stability", "Trend"
    );

    println!("{}", "────────────────────────────────────────────────────────────────────".dimmed());

    let mut stacks: Vec<PathBuf> = fs::read_dir(&history_dir)
        .map_err(|e| format!("Failed to read history dir: {}", e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();

    stacks.sort();

    for stack_path in stacks {

        let stack_name = stack_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        if let Some(latest) = load_latest(&stack_name) {

            let stability = calculate_stability(&stack_name, 5);

            let analysis = analyze_regression(
                &stack_name,
                latest.confidence,
                latest.readiness,
                latest.duration_seconds,
            );

            let readiness_value = latest.readiness.unwrap_or(0);

            let stack_col = format!("{:<20}", stack_name);
            let readiness_raw = format!("{:<12}", format!("{}%", readiness_value));
            let confidence_raw = format!("{:<12}", format!("{}%", latest.confidence));
            let risk_raw = format!("{:<12}", latest.risk);
            let stability_col = format!("{:<12}", format!("{}%", stability));

            let confidence_arrow = match analysis.confidence_trend.as_deref() {
    Some("UP") => "↑",
    Some("DOWN") => "↓",
    Some("SAME") => "→",
    _ => "-",
};

let readiness_arrow = match analysis.readiness_trend.as_deref() {
    Some("UP") => "↑",
    Some("DOWN") => "↓",
    Some("SAME") => "→",
    _ => "-",
};

let trend_combined = format!("C:{} R:{}", confidence_arrow, readiness_arrow);
let trend_raw = format!("{:<6}", trend_combined);

            let readiness_col = match readiness_value {
                90..=100 => readiness_raw.green(),
                70..=89 => readiness_raw.yellow(),
                40..=69 => readiness_raw.bright_red(),
                _ => readiness_raw.red(),
            };

            let confidence_col = match latest.confidence {
                90..=100 => confidence_raw.green(),
                70..=89 => confidence_raw.yellow(),
                40..=69 => confidence_raw.bright_red(),
                _ => confidence_raw.red(),
            };

            let risk_col = match latest.risk.as_str() {
                "LOW" => risk_raw.green(),
                "MODERATE" => risk_raw.yellow(),
                "HIGH" => risk_raw.bright_red(),
                "CRITICAL" => risk_raw.red(),
                _ => risk_raw.normal(),
            };

            let trend_col = match analysis.confidence_trend.as_deref() {
    Some("UP") => trend_raw.green(),
    Some("DOWN") => trend_raw.red(),
    Some("SAME") => trend_raw.yellow(),
    _ => trend_raw.normal(),
};

            println!(
                "{}{}{}{}{}{}",
                stack_col,
                readiness_col,
                confidence_col,
                risk_col,
                stability_col,
                trend_col
            );
        }
    }

    println!();
    Ok(())
}

// ======================================================
// REGRESSION ANALYSIS
// ======================================================

#[derive(Debug)]
pub struct RegressionAnalysis {
    pub previous_confidence: Option<u32>,
    pub confidence_delta: Option<i32>,
    pub confidence_trend: Option<String>,
    pub previous_readiness: Option<u32>,
    pub readiness_delta: Option<i32>,
    pub readiness_trend: Option<String>,
    pub duration_delta_percent: Option<i32>,
}

pub fn analyze_regression(
    stack: &str,
    current_confidence: u32,
    current_readiness: Option<u32>,
    current_duration: u64,
) -> RegressionAnalysis {

    let previous = load_latest(stack);

    if let Some(prev) = previous {

        let confidence_delta =
            current_confidence as i32 - prev.confidence as i32;

        let confidence_trend = if confidence_delta > 0 {
            "UP"
        } else if confidence_delta < 0 {
            "DOWN"
        } else {
            "SAME"
        };

        let readiness_delta = match (current_readiness, prev.readiness) {
            (Some(current), Some(previous)) =>
                Some(current as i32 - previous as i32),
            _ => None,
        };

        let readiness_trend = readiness_delta.map(|d| {
            if d > 0 {
                "UP".to_string()
            } else if d < 0 {
                "DOWN".to_string()
            } else {
                "SAME".to_string()
            }
        });

        let duration_delta_percent =
            if prev.duration_seconds > 0 {
                let diff =
                    current_duration as i64 - prev.duration_seconds as i64;

                Some(((diff * 100) / prev.duration_seconds as i64) as i32)
            } else {
                None
            };

        RegressionAnalysis {
            previous_confidence: Some(prev.confidence),
            confidence_delta: Some(confidence_delta),
            confidence_trend: Some(confidence_trend.to_string()),
            previous_readiness: prev.readiness,
            readiness_delta,
            readiness_trend,
            duration_delta_percent,
        }

    } else {

        RegressionAnalysis {
            previous_confidence: None,
            confidence_delta: None,
            confidence_trend: None,
            previous_readiness: None,
            readiness_delta: None,
            readiness_trend: None,
            duration_delta_percent: None,
        }
    }
}

use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

// ======================================================
// DATA STRUCTURES
// ======================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StackBaseline {
    pub stack: String,

    pub expected_services: Vec<String>,

    pub expected_confidence: u32,
    pub expected_readiness: Option<u32>,
    pub expected_duration: u64,

    pub service_scores: HashMap<String, u32>,
}

#[derive(Debug)]
pub struct BaselineDrift {
    pub new_services: Vec<String>,
    pub missing_services: Vec<String>,

    pub confidence_delta: i32,
    pub readiness_delta: Option<i32>,
    pub duration_delta_percent: Option<i32>,
}

// ======================================================
// PATH HELPERS
// ======================================================

fn baseline_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    Ok(home.join(".rehearsa").join("baselines"))
}

fn baseline_path(stack: &str) -> Result<PathBuf, String> {
    Ok(baseline_dir()?.join(format!("{}.json", stack)))
}

// ======================================================
// SAVE
// ======================================================

pub fn save_baseline(
    stack: &str,
    baseline: &StackBaseline,
) -> Result<(), String> {

    let dir = baseline_dir()?;

    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create baseline directory: {}", e))?;

    let path = baseline_path(stack)?;

    let json = serde_json::to_string_pretty(baseline)
        .map_err(|e| format!("Failed to serialize baseline: {}", e))?;

    fs::write(path, json)
        .map_err(|e| format!("Failed to write baseline file: {}", e))?;

    Ok(())
}

// ======================================================
// LOAD
// ======================================================

pub fn load_baseline(stack: &str) -> Option<StackBaseline> {

    let path = baseline_path(stack).ok()?;

    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(path).ok()?;

    serde_json::from_str(&content).ok()
}

// ======================================================
// DELETE
// ======================================================

pub fn delete_baseline(stack: &str) -> Result<(), String> {

    let path = baseline_path(stack)?;

    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path)
        .map_err(|e| format!("Failed to delete baseline: {}", e))?;

    Ok(())
}

// ======================================================
// DRIFT COMPARISON
// ======================================================

pub fn compare_to_baseline(
    baseline: &StackBaseline,
    current_services: &HashMap<String, u32>,
    current_confidence: u32,
    current_readiness: Option<u32>,
    current_duration: u64,
) -> BaselineDrift {

    // --------------------------------------------------
    // SERVICE DIFF
    // --------------------------------------------------

    let baseline_set: HashSet<_> =
        baseline.expected_services.iter().cloned().collect();

    let current_set: HashSet<_> =
        current_services.keys().cloned().collect();

    let new_services: Vec<String> =
        current_set.difference(&baseline_set).cloned().collect();

    let missing_services: Vec<String> =
        baseline_set.difference(&current_set).cloned().collect();

    // --------------------------------------------------
    // CONFIDENCE DELTA
    // --------------------------------------------------

    let confidence_delta =
        current_confidence as i32 - baseline.expected_confidence as i32;

    // --------------------------------------------------
    // READINESS DELTA
    // --------------------------------------------------

    let readiness_delta = match (current_readiness, baseline.expected_readiness) {
        (Some(current), Some(expected)) =>
            Some(current as i32 - expected as i32),
        _ => None,
    };

    // --------------------------------------------------
    // DURATION DELTA (%)
    // --------------------------------------------------

    let duration_delta_percent =
        if baseline.expected_duration > 0 {
            let diff = current_duration as i64
                - baseline.expected_duration as i64;

            Some(
                ((diff * 100)
                    / baseline.expected_duration as i64) as i32
            )
        } else {
            None
        };

    BaselineDrift {
        new_services,
        missing_services,
        confidence_delta,
        readiness_delta,
        duration_delta_percent,
    }
}

use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;

// ======================================================
// POLICY STRUCT
// ======================================================

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StackPolicy {
    pub min_confidence: Option<u32>,

    // NEW: Restore readiness enforcement
    pub min_readiness: Option<u32>,

    pub block_on_regression: Option<bool>,
    pub fail_on_new_service_failure: Option<bool>,

    // Duration-based enforcement
    pub fail_on_duration_spike: Option<bool>,
    pub duration_spike_percent: Option<u32>,
    pub fail_on_baseline_drift: Option<bool>,
}
// ======================================================
// INTERNAL PATH HELPERS
// ======================================================

fn policy_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    let dir = home.join(".rehearsa").join("policies");

    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create policy dir: {}", e))?;
    }

    Ok(dir)
}

fn policy_path(stack: &str) -> Result<PathBuf, String> {
    Ok(policy_dir()?.join(format!("{}.json", stack)))
}

// ======================================================
// LOAD
// ======================================================

pub fn load_policy(stack: &str) -> Option<StackPolicy> {
    let path = policy_path(stack).ok()?;

    if !path.exists() {
        return None;
    }

    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

// ======================================================
// SAVE (used by CLI Set)
// ======================================================

pub fn save_policy(stack: &str, policy: &StackPolicy) -> Result<(), String> {
    let path = policy_path(stack)?;

    let json = serde_json::to_string_pretty(policy)
        .map_err(|e| format!("Failed to serialize policy: {}", e))?;

    fs::write(path, json)
        .map_err(|e| format!("Failed to write policy file: {}", e))?;

    Ok(())
}

// ======================================================
// DELETE
// ======================================================

pub fn delete_policy(stack: &str) -> Result<(), String> {
    let path = policy_path(stack)?;

    if path.exists() {
        fs::remove_file(path)
            .map_err(|e| format!("Failed to delete policy: {}", e))?;
    }

    Ok(())
}

// ======================================================
// SHOW (used by CLI Show)
// ======================================================

pub fn show_policy(stack: &str) -> Result<(), String> {
    match load_policy(stack) {
        Some(policy) => {
            println!("{:#?}", policy);
        }
        None => {
            println!("No policy found for '{}'", stack);
        }
    }

    Ok(())
}

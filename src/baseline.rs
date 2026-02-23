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

    /// Timestamp of the run this baseline was pinned from.
    /// None for baselines created before this field was introduced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<String>,

    /// Wall-clock time at which this baseline was saved/promoted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promoted_at: Option<String>,
}

/// A single entry in the per-stack baseline history log.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BaselineHistoryEntry {
    /// Wall-clock time the baseline was saved.
    pub promoted_at: String,
    /// Timestamp of the run it was pinned from.
    pub pinned_at: Option<String>,
    pub expected_confidence: u32,
    pub expected_readiness: Option<u32>,
    pub expected_duration: u64,
    pub expected_services: Vec<String>,
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

fn baseline_history_dir(stack: &str) -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    Ok(home.join(".rehearsa").join("baseline-history").join(stack))
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

    // --------------------------------------------------
    // Append to baseline history log
    // --------------------------------------------------
    let promoted_at = baseline
        .promoted_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let entry = BaselineHistoryEntry {
        promoted_at:         promoted_at.clone(),
        pinned_at:           baseline.pinned_at.clone(),
        expected_confidence: baseline.expected_confidence,
        expected_readiness:  baseline.expected_readiness,
        expected_duration:   baseline.expected_duration,
        expected_services:   baseline.expected_services.clone(),
        service_scores:      baseline.service_scores.clone(),
    };

    let hist_dir = baseline_history_dir(stack)?;
    fs::create_dir_all(&hist_dir)
        .map_err(|e| format!("Failed to create baseline history directory: {}", e))?;

    let filename = format!("{}.json", promoted_at.replace(':', "-"));
    let hist_path = hist_dir.join(filename);

    let hist_json = serde_json::to_string_pretty(&entry)
        .map_err(|e| format!("Failed to serialize baseline history entry: {}", e))?;

    fs::write(hist_path, hist_json)
        .map_err(|e| format!("Failed to write baseline history entry: {}", e))?;

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
// PROMOTE
// ======================================================

/// Pin a historical run as the new baseline for a stack.
/// If `timestamp` is None, the latest run is used.
pub fn promote_baseline(stack: &str, timestamp: Option<&str>) -> Result<(), String> {

    let home = dirs::home_dir()
        .ok_or("Could not determine home directory")?;

    let stack_dir = home.join(".rehearsa").join("history").join(stack);

    if !stack_dir.exists() {
        return Err(format!(
            "No rehearsal history found for stack '{}'. Run a rehearsal first.",
            stack
        ));
    }

    // Collect and sort all history entries
    let mut entries: Vec<PathBuf> = fs::read_dir(&stack_dir)
        .map_err(|e| format!("Failed to read history directory: {}", e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();

    entries.sort();

    if entries.is_empty() {
        return Err(format!("No history entries found for stack '{}'.", stack));
    }

    // Find the target entry
    let target_path = if let Some(ts) = timestamp {
        // Match by timestamp prefix — the user might supply a partial timestamp
        let matched = entries.iter().find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.contains(&ts.replace(':', "-")))
                .unwrap_or(false)
        });

        matched
            .ok_or_else(|| format!(
                "No history entry found matching timestamp '{}' for stack '{}'.\n\
                 Run `rehearsa history show {}` to see available timestamps.",
                ts, stack, stack
            ))?
            .clone()
    } else {
        entries.last().unwrap().clone()
    };

    // Parse the run record
    let content = fs::read_to_string(&target_path)
        .map_err(|e| format!("Failed to read history entry: {}", e))?;

    let record: crate::history::RunRecord = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse history entry: {}", e))?;

    let now = chrono::Utc::now().to_rfc3339();

    let baseline = StackBaseline {
        stack:               stack.to_string(),
        expected_services:   record.services.keys().cloned().collect(),
        expected_confidence: record.confidence,
        expected_readiness:  record.readiness,
        expected_duration:   record.duration_seconds,
        service_scores:      record.services,
        pinned_at:           Some(record.timestamp.clone()),
        promoted_at:         Some(now),
    };

    save_baseline(stack, &baseline)?;

    println!("Baseline promoted for stack '{}'.", stack);
    println!("  Pinned from run : {}", record.timestamp);
    println!("  Confidence      : {}%", baseline.expected_confidence);
    if let Some(r) = baseline.expected_readiness {
        println!("  Readiness       : {}%", r);
    }
    println!("  Duration        : {}s", baseline.expected_duration);
    println!(
        "  Services        : {}",
        baseline.expected_services.join(", ")
    );

    Ok(())
}

// ======================================================
// BASELINE HISTORY
// ======================================================

/// Show baseline history for all stacks — one row per stack showing
/// the current pinned baseline and whether it has drifted from the latest run.
pub fn show_all_baseline_history() -> Result<(), String> {

    let baselines_dir = baseline_dir()?;

    if !baselines_dir.exists() {
        println!("No baselines found. Pin one with: rehearsa baseline set <compose-file>");
        return Ok(());
    }

    let mut stacks: Vec<String> = fs::read_dir(&baselines_dir)
        .map_err(|e| format!("Failed to read baselines directory: {}", e))?
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") {
                Some(name.trim_end_matches(".json").to_string())
            } else {
                None
            }
        })
        .collect();

    stacks.sort();

    if stacks.is_empty() {
        println!("No baselines found.");
        return Ok(());
    }

    println!();
    println!("Baseline History — All Stacks");
    println!("{}", "─".repeat(90));
    println!(
        "{:<20} {:<26} {:<12} {:<12} {:<12} {}",
        "Stack", "Promoted At", "Confidence", "Readiness", "Duration", "Status"
    );
    println!("{}", "─".repeat(90));

    for stack in &stacks {
        let baseline = match load_baseline(stack) {
            Some(b) => b,
            None    => continue,
        };

        let promoted = baseline
            .promoted_at
            .as_deref()
            .map(|t| if t.len() >= 19 { &t[..19] } else { t })
            .unwrap_or("unknown")
            .replace('T', " ");

        // Compare against latest run to show drift status
        let status = if let Some(latest) = crate::history::load_latest(stack) {
            let drift = compare_to_baseline(
                &baseline,
                &latest.services,
                latest.confidence,
                latest.readiness,
                latest.duration_seconds,
            );

            let has_drift = !drift.new_services.is_empty()
                || !drift.missing_services.is_empty()
                || drift.confidence_delta != 0
                || drift.readiness_delta.unwrap_or(0) != 0
                || drift.duration_delta_percent.unwrap_or(0) != 0;

            if has_drift { "DRIFT DETECTED" } else { "CONTRACT HONOURED" }
        } else {
            "NO RUNS"
        };

        // Count versions in history log
        let version_count = baseline_history_dir(stack)
            .ok()
            .and_then(|d| fs::read_dir(d).ok())
            .map(|entries| entries.filter_map(|e| e.ok()).count())
            .unwrap_or(1);

        println!(
            "{:<20} {:<26} {:<12} {:<12} {:<12} {}  ({} version{})",
            stack,
            promoted,
            format!("{}%", baseline.expected_confidence),
            baseline.expected_readiness
                .map(|r| format!("{}%", r))
                .unwrap_or_else(|| "—".to_string()),
            format!("{}s", baseline.expected_duration),
            status,
            version_count,
            if version_count == 1 { "" } else { "s" },
        );
    }

    println!();
    Ok(())
}

/// Show the full baseline version history for a single stack,
/// with a diff between each consecutive version.
pub fn show_stack_baseline_history(stack: &str) -> Result<(), String> {

    let hist_dir = baseline_history_dir(stack)?;

    if !hist_dir.exists() {
        // Fall back gracefully — if there's a current baseline but no history
        // dir (created before this feature), surface what we have.
        if let Some(b) = load_baseline(stack) {
            println!("Stack: {}", stack);
            println!("{}", "─".repeat(60));
            println!("1 baseline on record (history logging introduced in 0.8.0).");
            println!();
            println!(
                "  Confidence : {}%",
                b.expected_confidence
            );
            if let Some(r) = b.expected_readiness {
                println!("  Readiness  : {}%", r);
            }
            println!("  Duration   : {}s", b.expected_duration);
            println!("  Services   : {}", b.expected_services.join(", "));
            return Ok(());
        }

        return Err(format!(
            "No baseline history found for stack '{}'. Pin a baseline first.",
            stack
        ));
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(&hist_dir)
        .map_err(|e| format!("Failed to read baseline history: {}", e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();

    entries.sort();

    if entries.is_empty() {
        return Err(format!("No baseline history entries found for '{}'.", stack));
    }

    println!();
    println!("Baseline History: {}", stack);
    println!("{}", "─".repeat(70));
    println!("{} version{} on record.\n", entries.len(), if entries.len() == 1 { "" } else { "s" });

    let parsed: Vec<BaselineHistoryEntry> = entries
        .iter()
        .filter_map(|p| {
            let content = fs::read_to_string(p).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect();

    for (i, entry) in parsed.iter().enumerate() {
        let promoted = if entry.promoted_at.len() >= 19 {
            entry.promoted_at[..19].replace('T', " ")
        } else {
            entry.promoted_at.clone()
        };

        let pinned = entry.pinned_at
            .as_deref()
            .map(|t| if t.len() >= 19 { t[..19].replace('T', " ") } else { t.to_string() })
            .unwrap_or_else(|| "unknown".to_string());

        println!(
            "v{}  Promoted: {}  (from run: {})",
            i + 1,
            promoted,
            pinned,
        );
        println!(
            "    Confidence: {}%   Readiness: {}   Duration: {}s",
            entry.expected_confidence,
            entry.expected_readiness
                .map(|r| format!("{}%", r))
                .unwrap_or_else(|| "—".to_string()),
            entry.expected_duration,
        );
        println!(
            "    Services: {}",
            entry.expected_services.join(", ")
        );

        // Show diff from previous version
        if i > 0 {
            let prev = &parsed[i - 1];
            let mut diffs: Vec<String> = vec![];

            let conf_delta = entry.expected_confidence as i32 - prev.expected_confidence as i32;
            if conf_delta != 0 {
                diffs.push(format!("confidence {:+}%", conf_delta));
            }

            if let (Some(cur_r), Some(prev_r)) = (entry.expected_readiness, prev.expected_readiness) {
                let r_delta = cur_r as i32 - prev_r as i32;
                if r_delta != 0 {
                    diffs.push(format!("readiness {:+}%", r_delta));
                }
            }

            if prev.expected_duration > 0 {
                let dur_delta = (entry.expected_duration as i64 - prev.expected_duration as i64)
                    * 100 / prev.expected_duration as i64;
                if dur_delta != 0 {
                    diffs.push(format!("duration {:+}%", dur_delta));
                }
            }

            let prev_set: HashSet<_> = prev.expected_services.iter().collect();
            let cur_set:  HashSet<_> = entry.expected_services.iter().collect();
            for added   in cur_set.difference(&prev_set)   { diffs.push(format!("+{}", added)); }
            for removed in prev_set.difference(&cur_set)   { diffs.push(format!("-{}", removed)); }

            if diffs.is_empty() {
                println!("    Δ from v{}: no changes", i);
            } else {
                println!("    Δ from v{}: {}", i, diffs.join(", "));
            }
        }

        println!();
    }

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

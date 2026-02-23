use serde::{Serialize, Deserialize};

use crate::baseline::{load_baseline, compare_to_baseline};
use crate::daemon::load_registry;
use crate::history::load_latest;

// ======================================================
// DATA MODEL
// ======================================================

/// Coverage status for a single stack.
#[derive(Debug, Serialize, Deserialize)]
pub struct StackCoverage {
    pub stack:           String,
    /// Whether this stack is registered in the watch registry.
    pub watched:         bool,
    /// Whether a baseline contract has been pinned.
    pub has_baseline:    bool,
    /// Whether at least one rehearsal has been run.
    pub has_history:     bool,
    /// CONTRACT_HONOURED | DRIFT_DETECTED | NO_RUNS | NO_BASELINE | UNWATCHED
    pub status:          String,
    /// Latest confidence score. None if no history.
    pub confidence:      Option<u32>,
    /// Latest readiness score. None if no history.
    pub readiness:       Option<u32>,
}

/// Fleet-wide coverage summary.
#[derive(Debug, Serialize, Deserialize)]
pub struct CoverageSummary {
    pub total_watched:         usize,
    /// Stacks with a baseline pinned.
    pub with_baseline:         usize,
    /// Stacks with a baseline AND currently honouring it.
    pub honouring_contract:    usize,
    /// Stacks with history but no baseline — running blind.
    pub uncontracted:          usize,
    /// Stacks with no rehearsal history at all.
    pub never_rehearsed:       usize,
    /// 0–100: percentage of watched stacks honouring their contract.
    pub coverage_pct:          u32,
    pub stacks:                Vec<StackCoverage>,
}

// ======================================================
// CORE LOGIC
// ======================================================

pub fn build_coverage() -> Result<CoverageSummary, String> {
    let registry = load_registry()?;
    let watches = &registry.watches;

    if watches.is_empty() {
        return Ok(CoverageSummary {
            total_watched:      0,
            with_baseline:      0,
            honouring_contract: 0,
            uncontracted:       0,
            never_rehearsed:    0,
            coverage_pct:       0,
            stacks:             vec![],
        });
    }

    let mut stacks: Vec<StackCoverage> = Vec::new();

    for watch in watches {
        let stack = &watch.stack;

        let latest   = load_latest(stack);
        let baseline = load_baseline(stack);

        let has_history  = latest.is_some();
        let has_baseline = baseline.is_some();

        let (status, confidence, readiness) = match (&latest, &baseline) {
            (None, _) => (
                "NO_RUNS".to_string(),
                None,
                None,
            ),
            (Some(run), None) => (
                "NO_BASELINE".to_string(),
                Some(run.confidence),
                run.readiness,
            ),
            (Some(run), Some(bl)) => {
                let drift = compare_to_baseline(
                    bl,
                    &run.services,
                    run.confidence,
                    run.readiness,
                    run.duration_seconds,
                );

                let has_drift = !drift.new_services.is_empty()
                    || !drift.missing_services.is_empty()
                    || drift.confidence_delta < -5        // tolerate tiny float noise
                    || drift.readiness_delta.unwrap_or(0) < -5
                    || drift.duration_delta_percent.unwrap_or(0).abs() > 20;

                let status = if has_drift {
                    "DRIFT_DETECTED".to_string()
                } else {
                    "CONTRACT_HONOURED".to_string()
                };

                (status, Some(run.confidence), run.readiness)
            }
        };

        stacks.push(StackCoverage {
            stack:        stack.clone(),
            watched:      true,
            has_baseline,
            has_history,
            status,
            confidence,
            readiness,
        });
    }

    // ──────────────────────────────────────────────
    // Aggregate
    // ──────────────────────────────────────────────

    let total_watched      = stacks.len();
    let with_baseline      = stacks.iter().filter(|s| s.has_baseline).count();
    let honouring_contract = stacks.iter().filter(|s| s.status == "CONTRACT_HONOURED").count();
    let uncontracted       = stacks.iter().filter(|s| s.has_history && !s.has_baseline).count();
    let never_rehearsed    = stacks.iter().filter(|s| !s.has_history).count();

    let coverage_pct = if total_watched > 0 {
        ((honouring_contract * 100) / total_watched) as u32
    } else {
        0
    };

    // Sort: honouring first, then drift, then no baseline, then no runs
    stacks.sort_by_key(|s| match s.status.as_str() {
        "CONTRACT_HONOURED" => 0,
        "DRIFT_DETECTED"    => 1,
        "NO_BASELINE"       => 2,
        _                   => 3,
    });

    Ok(CoverageSummary {
        total_watched,
        with_baseline,
        honouring_contract,
        uncontracted,
        never_rehearsed,
        coverage_pct,
        stacks,
    })
}

// ======================================================
// DISPLAY
// ======================================================

pub fn print_coverage(summary: &CoverageSummary) {
    println!();
    println!("Restore Contract Coverage");
    println!("{}", "─".repeat(60));

    if summary.total_watched == 0 {
        println!("No stacks are being watched.");
        println!("Add a stack with: rehearsa daemon watch <stack> <compose-file>");
        println!();
        return;
    }

    // ── Headline ──────────────────────────────────
    let bar = coverage_bar(summary.coverage_pct);
    println!("Coverage  {}  {}%", bar, summary.coverage_pct);
    println!();

    // ── Fleet counters ────────────────────────────
    println!(
        "  {:>3}  watched",
        summary.total_watched
    );
    println!(
        "  {:>3}  with baseline contract",
        summary.with_baseline
    );
    println!(
        "  {:>3}  honouring contract  ✓",
        summary.honouring_contract
    );

    if summary.uncontracted > 0 {
        println!(
            "  {:>3}  rehearsed but no baseline  ⚠",
            summary.uncontracted
        );
    }
    if summary.never_rehearsed > 0 {
        println!(
            "  {:>3}  never rehearsed  ✗",
            summary.never_rehearsed
        );
    }

    // ── Per-stack table ───────────────────────────
    println!();
    println!(
        "{:<22} {:<20} {:>10} {:>10}",
        "Stack", "Status", "Confidence", "Readiness"
    );
    println!("{}", "─".repeat(66));

    for s in &summary.stacks {
        let conf_str = s.confidence
            .map(|c| format!("{}%", c))
            .unwrap_or_else(|| "—".to_string());

        let read_str = s.readiness
            .map(|r| format!("{}%", r))
            .unwrap_or_else(|| "—".to_string());

        let status_icon = match s.status.as_str() {
            "CONTRACT_HONOURED" => "✓  CONTRACT HONOURED",
            "DRIFT_DETECTED"    => "⚠  DRIFT DETECTED",
            "NO_BASELINE"       => "·  NO BASELINE",
            "NO_RUNS"           => "✗  NEVER REHEARSED",
            _                   => &s.status,
        };

        println!(
            "{:<22} {:<20} {:>10} {:>10}",
            s.stack, status_icon, conf_str, read_str
        );
    }

    println!();

    // ── Guidance ─────────────────────────────────
    if summary.never_rehearsed > 0 {
        println!("  Run rehearsals:  rehearsa stack test <compose-file>");
    }
    if summary.uncontracted > 0 {
        println!("  Pin contracts:   rehearsa baseline set <compose-file>");
    }
    if summary.coverage_pct == 100 {
        println!("  All contracts are honoured.");
    }

    println!();
}

/// ASCII coverage bar, 20 characters wide.
fn coverage_bar(pct: u32) -> String {
    let filled = ((pct as usize) * 20) / 100;
    let empty  = 20 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

// ======================================================
// JSON OUTPUT
// ======================================================

pub fn print_coverage_json(summary: &CoverageSummary) -> Result<(), String> {
    let json = serde_json::to_string_pretty(summary)
        .map_err(|e| format!("JSON error: {}", e))?;
    println!("{}", json);
    Ok(())
}

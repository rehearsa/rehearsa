use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::baseline::load_baseline;
use crate::history::{calculate_stability, load_latest, RunRecord};
use crate::policy::load_policy;
use crate::provider::load_provider;

// ======================================================
// REPORT DATA MODEL
// ======================================================

/// Top-level compliance report. Serialises directly to JSON.
/// The PDF renderer walks this same structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct ComplianceReport {
    pub meta:      ReportMeta,
    pub summary:   ReportSummary,
    pub rehearsal: RehearsalSection,
    pub history:   HistorySection,
    pub baseline:  BaselineSection,
    pub policy:    PolicySection,
    pub preflight: PreflightSection,
    pub provider:  ProviderSection,
}

// ──────────────────────────────────────────────────────
// META
// ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportMeta {
    /// "stack" | "fleet"
    pub scope:            String,
    /// Stack name, or "fleet" for multi-stack reports.
    pub target:           String,
    pub generated_at:     String,
    pub rehearsa_version: String,
    pub report_id:        String,
}

// ──────────────────────────────────────────────────────
// SUMMARY (headline verdict)
// ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportSummary {
    /// "PASS" | "FAIL" | "WARN"
    pub verdict:          String,
    pub confidence:       u32,
    pub readiness:        u32,
    pub risk:             String,
    pub stability:        u32,
    pub policy_violated:  bool,
    pub baseline_drift:   bool,
    pub provider_ok:      Option<bool>,
}

// ──────────────────────────────────────────────────────
// REHEARSAL (latest run)
// ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct RehearsalSection {
    pub timestamp:        String,
    pub duration_seconds: u64,
    pub confidence:       u32,
    pub readiness:        u32,
    pub risk:             String,
    pub exit_code:        i32,
    pub services:         HashMap<String, u32>,
}

// ──────────────────────────────────────────────────────
// HISTORY (trend window)
// ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct HistorySection {
    /// Number of runs included.
    pub window:     usize,
    pub stability:  u32,
    /// Trend direction derived from last two runs: "UP" | "DOWN" | "SAME" | "INSUFFICIENT_DATA"
    pub trend:      String,
    pub runs:       Vec<HistoryRun>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryRun {
    pub timestamp:        String,
    pub confidence:       u32,
    pub readiness:        Option<u32>,
    pub duration_seconds: u64,
    pub risk:             String,
    pub exit_code:        i32,
}

// ──────────────────────────────────────────────────────
// BASELINE CONTRACT
// ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct BaselineSection {
    /// true if a baseline has been pinned for this stack.
    pub pinned:              bool,
    /// "CONTRACT_HONOURED" | "DRIFT_DETECTED" | "NO_BASELINE"
    pub status:              String,
    pub expected_confidence: Option<u32>,
    pub expected_readiness:  Option<u32>,
    pub expected_duration:   Option<u64>,
    pub expected_services:   Vec<String>,
    pub confidence_delta:    Option<i32>,
    pub readiness_delta:     Option<i32>,
    pub duration_delta_pct:  Option<i32>,
    pub new_services:        Vec<String>,
    pub missing_services:    Vec<String>,
}

// ──────────────────────────────────────────────────────
// POLICY
// ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicySection {
    pub configured:          bool,
    /// "PASS" | "FAIL" | "NOT_CONFIGURED"
    pub verdict:             String,
    pub checks:              Vec<PolicyCheck>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyCheck {
    pub rule:    String,
    pub setting: String,
    /// "PASS" | "FAIL" | "SKIP"
    pub result:  String,
    pub detail:  String,
}

// ──────────────────────────────────────────────────────
// PREFLIGHT
// ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct PreflightSection {
    pub restore_readiness_score: u32,
    pub findings:                Vec<PreflightFinding>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreflightFinding {
    /// "CRITICAL" | "WARNING" | "INFO"
    pub severity: String,
    pub message:  String,
}

// ──────────────────────────────────────────────────────
// PROVIDER
// ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderSection {
    pub attached:   bool,
    pub name:       Option<String>,
    pub kind:       Option<String>,
    pub repository: Option<String>,
    /// None = not checked, Some(true) = verified OK, Some(false) = verification failed
    pub verified:   Option<bool>,
}

// ======================================================
// HISTORY LOADER
// ======================================================

fn load_history(stack: &str, window: usize) -> Vec<RunRecord> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };

    let stack_dir = home
        .join(".rehearsa")
        .join("history")
        .join(stack);

    if !stack_dir.exists() {
        return vec![];
    }

    let mut entries: Vec<PathBuf> = match fs::read_dir(&stack_dir) {
        Ok(e) => e.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return vec![],
    };

    entries.sort();

    entries
        .into_iter()
        .rev()
        .take(window)
        .rev()
        .filter_map(|p| {
            let content = fs::read_to_string(p).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect()
}

// ======================================================
// REPORT BUILDER
// ======================================================

pub struct ReportOptions {
    pub stack:          String,
    /// How many historical runs to include. Default: 10.
    pub history_window: usize,
    /// If set, include provider status for this named provider.
    pub provider_name:  Option<String>,
}

/// Build a ComplianceReport from on-disk state. Pure data assembly — no Docker calls.
/// Returns Err if there is no history at all for the stack (nothing to report).
pub fn build_report(opts: &ReportOptions) -> Result<ComplianceReport, String> {
    let stack = &opts.stack;

    // ──────────────────────────────────────────────
    // Latest run — required; fail fast if absent
    // ──────────────────────────────────────────────
    let latest = load_latest(stack)
        .ok_or_else(|| format!("No rehearsal history found for stack '{}'.", stack))?;

    // ──────────────────────────────────────────────
    // Meta
    // ──────────────────────────────────────────────
    let report_id = uuid_v4_hex();
    let meta = ReportMeta {
        scope:            "stack".to_string(),
        target:           stack.clone(),
        generated_at:     chrono::Utc::now().to_rfc3339(),
        rehearsa_version: env!("CARGO_PKG_VERSION").to_string(),
        report_id,
    };

    // ──────────────────────────────────────────────
    // Rehearsal section
    // ──────────────────────────────────────────────
    let rehearsal = RehearsalSection {
        timestamp:        latest.timestamp.clone(),
        duration_seconds: latest.duration_seconds,
        confidence:       latest.confidence,
        readiness:        latest.readiness.unwrap_or(0),
        risk:             latest.risk.clone(),
        exit_code:        latest.exit_code,
        services:         latest.services.clone(),
    };

    // ──────────────────────────────────────────────
    // History section
    // ──────────────────────────────────────────────
    let history_records = load_history(stack, opts.history_window);
    let stability = calculate_stability(stack, 5);

    let trend = {
        let len = history_records.len();
        if len < 2 {
            "INSUFFICIENT_DATA".to_string()
        } else {
            let prev = history_records[len - 2].confidence as i32;
            let curr = history_records[len - 1].confidence as i32;
            let delta = curr - prev;
            if delta > 0 { "UP" } else if delta < 0 { "DOWN" } else { "SAME" }.to_string()
        }
    };

    let runs: Vec<HistoryRun> = history_records
        .iter()
        .map(|r| HistoryRun {
            timestamp:        r.timestamp.clone(),
            confidence:       r.confidence,
            readiness:        r.readiness,
            duration_seconds: r.duration_seconds,
            risk:             r.risk.clone(),
            exit_code:        r.exit_code,
        })
        .collect();

    let history = HistorySection {
        window:    runs.len(),
        stability,
        trend,
        runs,
    };

    // ──────────────────────────────────────────────
    // Baseline section
    // ──────────────────────────────────────────────
    let baseline_section = if let Some(baseline) = load_baseline(stack) {
        use crate::baseline::compare_to_baseline;

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

        BaselineSection {
            pinned:              true,
            status:              if has_drift {
                "DRIFT_DETECTED".to_string()
            } else {
                "CONTRACT_HONOURED".to_string()
            },
            expected_confidence: Some(baseline.expected_confidence),
            expected_readiness:  baseline.expected_readiness,
            expected_duration:   Some(baseline.expected_duration),
            expected_services:   baseline.expected_services.clone(),
            confidence_delta:    Some(drift.confidence_delta),
            readiness_delta:     drift.readiness_delta,
            duration_delta_pct:  drift.duration_delta_percent,
            new_services:        drift.new_services,
            missing_services:    drift.missing_services,
        }
    } else {
        BaselineSection {
            pinned:              false,
            status:              "NO_BASELINE".to_string(),
            expected_confidence: None,
            expected_readiness:  None,
            expected_duration:   None,
            expected_services:   vec![],
            confidence_delta:    None,
            readiness_delta:     None,
            duration_delta_pct:  None,
            new_services:        vec![],
            missing_services:    vec![],
        }
    };

    let baseline_drift = baseline_section.status == "DRIFT_DETECTED";

    // ──────────────────────────────────────────────
    // Policy section
    // ──────────────────────────────────────────────
    let policy_section = if let Some(policy) = load_policy(stack) {
        let mut checks: Vec<PolicyCheck> = vec![];
        let mut any_fail = false;

        // min_confidence
        if let Some(min) = policy.min_confidence {
            let pass = latest.confidence >= min;
            if !pass { any_fail = true; }
            checks.push(PolicyCheck {
                rule:    "min_confidence".to_string(),
                setting: format!("{}%", min),
                result:  if pass { "PASS" } else { "FAIL" }.to_string(),
                detail:  format!("actual {}%", latest.confidence),
            });
        }

        // min_readiness
        if let Some(min) = policy.min_readiness {
            let actual = latest.readiness.unwrap_or(0);
            let pass = actual >= min;
            if !pass { any_fail = true; }
            checks.push(PolicyCheck {
                rule:    "min_readiness".to_string(),
                setting: format!("{}%", min),
                result:  if pass { "PASS" } else { "FAIL" }.to_string(),
                detail:  format!("actual {}%", actual),
            });
        }

        // block_on_regression
        if policy.block_on_regression.unwrap_or(false) {
            let regressed = history.trend == "DOWN";
            if regressed { any_fail = true; }
            checks.push(PolicyCheck {
                rule:    "block_on_regression".to_string(),
                setting: "true".to_string(),
                result:  if regressed { "FAIL" } else { "PASS" }.to_string(),
                detail:  format!("trend: {}", history.trend),
            });
        }

        // fail_on_baseline_drift
        if policy.fail_on_baseline_drift.unwrap_or(false) {
            if baseline_drift { any_fail = true; }
            checks.push(PolicyCheck {
                rule:    "fail_on_baseline_drift".to_string(),
                setting: "true".to_string(),
                result:  if baseline_drift { "FAIL" } else { "PASS" }.to_string(),
                detail:  baseline_section.status.clone(),
            });
        }

        // fail_on_new_service_failure
        if policy.fail_on_new_service_failure.unwrap_or(false) {
            let failed: Vec<&str> = latest.services
                .iter()
                .filter(|(_, &s)| s == 0)
                .map(|(n, _)| n.as_str())
                .collect();
            let pass = failed.is_empty();
            if !pass { any_fail = true; }
            checks.push(PolicyCheck {
                rule:    "fail_on_new_service_failure".to_string(),
                setting: "true".to_string(),
                result:  if pass { "PASS" } else { "FAIL" }.to_string(),
                detail:  if pass {
                    "all services started".to_string()
                } else {
                    format!("failed: {}", failed.join(", "))
                },
            });
        }

        PolicySection {
            configured: true,
            verdict:    if any_fail { "FAIL" } else { "PASS" }.to_string(),
            checks,
        }
    } else {
        PolicySection {
            configured: false,
            verdict:    "NOT_CONFIGURED".to_string(),
            checks:     vec![],
        }
    };

    let policy_violated = policy_section.verdict == "FAIL";

    // ──────────────────────────────────────────────
    // Preflight section — reconstructed from latest run metadata.
    // Full preflight findings are not stored in RunRecord (by design —
    // they are ephemeral output). We surface the restore readiness score
    // that was captured and note that detailed findings are live-only.
    // ──────────────────────────────────────────────
    let preflight_section = PreflightSection {
        restore_readiness_score: latest.readiness.unwrap_or(0),
        findings: vec![PreflightFinding {
            severity: "INFO".to_string(),
            message:  "Detailed preflight findings are captured at rehearsal runtime. \
                       Re-run `rehearsa run` to view live findings."
                .to_string(),
        }],
    };

    // ──────────────────────────────────────────────
    // Provider section
    // ──────────────────────────────────────────────
    let provider_section = if let Some(ref pname) = opts.provider_name {
        if let Some(provider) = load_provider(pname) {
            ProviderSection {
                attached:   true,
                name:       Some(provider.name.clone()),
                kind:       Some(provider.kind.to_string()),
                repository: Some(provider.repository.clone()),
                // Verification status is not stored persistently — we report
                // "not checked at report time" so users know to run provider verify.
                verified:   None,
            }
        } else {
            ProviderSection {
                attached:   true,
                name:       Some(pname.clone()),
                kind:       None,
                repository: None,
                verified:   Some(false),
            }
        }
    } else {
        ProviderSection {
            attached:   false,
            name:       None,
            kind:       None,
            repository: None,
            verified:   None,
        }
    };

    // ──────────────────────────────────────────────
    // Summary verdict
    // ──────────────────────────────────────────────
    let verdict = if policy_violated || baseline_drift {
        "FAIL"
    } else if latest.confidence < 70 {
        "FAIL"
    } else if latest.confidence < 90 || history.trend == "DOWN" {
        "WARN"
    } else {
        "PASS"
    }
    .to_string();

    let summary = ReportSummary {
        verdict,
        confidence:      latest.confidence,
        readiness:       latest.readiness.unwrap_or(0),
        risk:            latest.risk.clone(),
        stability,
        policy_violated,
        baseline_drift,
        provider_ok:     provider_section.verified,
    };

    Ok(ComplianceReport {
        meta,
        summary,
        rehearsal,
        history,
        baseline: baseline_section,
        policy:   policy_section,
        preflight: preflight_section,
        provider:  provider_section,
    })
}

// ======================================================
// FLEET REPORT
// ======================================================

/// Build one report per known stack and collect them.
pub fn build_fleet_report() -> Vec<ComplianceReport> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };

    let history_dir = home.join(".rehearsa").join("history");
    if !history_dir.exists() {
        return vec![];
    }

    let mut stacks: Vec<String> = match fs::read_dir(&history_dir) {
        Ok(entries) => entries
            .filter_map(|e| {
                let e = e.ok()?;
                if e.path().is_dir() {
                    Some(e.file_name().to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect(),
        Err(_) => return vec![],
    };

    stacks.sort();

    stacks
        .into_iter()
        .filter_map(|stack| {
            let opts = ReportOptions {
                stack:          stack.clone(),
                history_window: 10,
                provider_name:  None,
            };
            build_report(&opts).ok()
        })
        .collect()
}

// ======================================================
// JSON OUTPUT
// ======================================================

pub fn render_json(report: &ComplianceReport) -> Result<String, String> {
    serde_json::to_string_pretty(report)
        .map_err(|e| format!("JSON serialisation failed: {}", e))
}

pub fn render_json_fleet(reports: &[ComplianceReport]) -> Result<String, String> {
    serde_json::to_string_pretty(reports)
        .map_err(|e| format!("JSON serialisation failed: {}", e))
}

// ======================================================
// PDF OUTPUT
// ======================================================

/// Render a single-stack report to PDF bytes using printpdf.
pub fn render_pdf(report: &ComplianceReport) -> Result<Vec<u8>, String> {
    use printpdf::*;

    let (doc, page1, layer1) = PdfDocument::new(
        format!("Rehearsa Compliance Report — {}", report.meta.target),
        Mm(210.0),
        Mm(297.0),
        "Layer 1",
    );

    let mut page_idx  = page1;
    let mut layer_idx = layer1;

    // Font
    let font = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("Font error: {}", e))?;
    let font_regular = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("Font error: {}", e))?;
    let font_mono = doc
        .add_builtin_font(BuiltinFont::Courier)
        .map_err(|e| format!("Font error: {}", e))?;

    // Margins
    let left_margin = Mm(20.0);
    let right_margin = Mm(190.0);
    let top_start = Mm(277.0);
    let bottom_margin = Mm(20.0);
    let line_height_lg = Mm(8.0);
    let line_height_md = Mm(6.0);
    let line_height_sm = Mm(5.0);

    let mut y = top_start;

    macro_rules! current_layer {
        () => {
            doc.get_page(page_idx).get_layer(layer_idx)
        };
    }

    macro_rules! new_page_if_needed {
        ($needed:expr) => {
            if y < bottom_margin + $needed {
                let (np, nl) = doc.add_page(Mm(210.0), Mm(297.0), "Layer 1");
                page_idx  = np;
                layer_idx = nl;
                y = top_start;
            }
        };
    }

    // ── Helper: draw a full-width horizontal rule ──────────────────────────
    let draw_rule = |layer: &PdfLayerReference, y_pos: Mm| {
        let line = Line {
            points: vec![
                (Point::new(left_margin, y_pos), false),
                (Point::new(right_margin, y_pos), false),
            ],
            is_closed: false,
        };
        layer.add_line(line);
    };

    // ── Helper: coloured verdict badge ────────────────────────────────────
    let verdict_color = match report.summary.verdict.as_str() {
        "PASS" => Color::Rgb(Rgb::new(0.13, 0.60, 0.33, None)),
        "WARN" => Color::Rgb(Rgb::new(0.85, 0.55, 0.05, None)),
        _      => Color::Rgb(Rgb::new(0.80, 0.15, 0.15, None)),
    };

    // ══════════════════════════════════════════════
    // HEADER
    // ══════════════════════════════════════════════
    {
        let layer = current_layer!();

        layer.set_fill_color(Color::Rgb(Rgb::new(0.10, 0.10, 0.10, None)));
        layer.use_text("Rehearsa", 22.0, left_margin, y, &font);
        layer.use_text(
            "Compliance Report",
            14.0,
            Mm(68.0),
            y,
            &font_regular,
        );

        y -= line_height_lg;
        draw_rule(&layer, y);
        y -= line_height_md;

        layer.use_text(
            &format!("Stack: {}   |   Generated: {}   |   ID: {}",
                report.meta.target,
                &report.meta.generated_at[..19].replace('T', " "),
                &report.meta.report_id[..8],
            ),
            8.0,
            left_margin,
            y,
            &font_regular,
        );

        y -= line_height_lg;
    }

    // ══════════════════════════════════════════════
    // VERDICT BANNER
    // ══════════════════════════════════════════════
    {
        let layer = current_layer!();

        // Banner background rect
        let banner_top    = y + Mm(2.0);
        let banner_bottom = y - Mm(10.0);

        let banner_color = match report.summary.verdict.as_str() {
            "PASS" => Color::Rgb(Rgb::new(0.90, 0.97, 0.92, None)),
            "WARN" => Color::Rgb(Rgb::new(1.0,  0.97, 0.88, None)),
            _      => Color::Rgb(Rgb::new(0.99, 0.91, 0.91, None)),
        };

        layer.set_fill_color(banner_color);
        layer.add_rect(Rect::new(
            left_margin,
            banner_bottom,
            right_margin,
            banner_top,
        ));

        layer.set_fill_color(verdict_color.clone());
        layer.use_text(
            &format!("▐  {}  — Confidence: {}%   Readiness: {}%   Risk: {}   Stability: {}%",
                report.summary.verdict,
                report.summary.confidence,
                report.summary.readiness,
                report.summary.risk,
                report.summary.stability,
            ),
            11.0,
            left_margin + Mm(2.0),
            y - Mm(4.0),
            &font,
        );

        y -= Mm(14.0);
    }

    // ══════════════════════════════════════════════
    // SECTION HELPER MACRO
    // ══════════════════════════════════════════════
    macro_rules! section_heading {
        ($title:expr) => {{
            new_page_if_needed!(Mm(20.0));
            y -= line_height_md;
            let layer = current_layer!();
            layer.set_fill_color(Color::Rgb(Rgb::new(0.10, 0.10, 0.10, None)));
            layer.use_text($title, 11.0, left_margin, y, &font);
            y -= Mm(1.5);
            draw_rule(&current_layer!(), y);
            y -= line_height_sm;
        }};
    }

    macro_rules! kv_line {
        ($label:expr, $value:expr) => {{
            new_page_if_needed!(line_height_sm);
            let layer = current_layer!();
            layer.set_fill_color(Color::Rgb(Rgb::new(0.35, 0.35, 0.35, None)));
            layer.use_text($label, 8.5, left_margin + Mm(2.0), y, &font);
            layer.set_fill_color(Color::Rgb(Rgb::new(0.10, 0.10, 0.10, None)));
            layer.use_text($value, 8.5, Mm(80.0), y, &font_regular);
            y -= line_height_sm;
        }};
    }

    macro_rules! mono_line {
        ($text:expr) => {{
            new_page_if_needed!(line_height_sm);
            let layer = current_layer!();
            layer.set_fill_color(Color::Rgb(Rgb::new(0.20, 0.20, 0.20, None)));
            layer.use_text($text, 7.5, left_margin + Mm(2.0), y, &font_mono);
            y -= line_height_sm;
        }};
    }

    // ══════════════════════════════════════════════
    // 1. LATEST REHEARSAL
    // ══════════════════════════════════════════════
    section_heading!("1. Latest Rehearsal");
    kv_line!("Timestamp",        &report.rehearsal.timestamp[..19].replace('T', " "));
    kv_line!("Duration",         &format!("{}s", report.rehearsal.duration_seconds));
    kv_line!("Confidence",       &format!("{}%", report.rehearsal.confidence));
    kv_line!("Readiness",        &format!("{}%", report.rehearsal.readiness));
    kv_line!("Risk Band",        &report.rehearsal.risk);
    kv_line!("Exit Code",        &report.rehearsal.exit_code.to_string());

    y -= line_height_sm;
    {
        let layer = current_layer!();
        layer.set_fill_color(Color::Rgb(Rgb::new(0.35, 0.35, 0.35, None)));
        layer.use_text("Service Scores", 8.5, left_margin + Mm(2.0), y, &font);
        y -= line_height_sm;
    }

    let mut services: Vec<(&String, &u32)> = report.rehearsal.services.iter().collect();
    services.sort_by_key(|(k, _)| k.as_str());
    for (name, score) in &services {
        let bar = score_bar(**score);
        mono_line!(&format!("  {:<24} {:>3}%  {}", name, score, bar));
    }

    // ══════════════════════════════════════════════
    // 2. HISTORY & TREND
    // ══════════════════════════════════════════════
    section_heading!("2. History & Trend");
    kv_line!("Window",    &format!("{} runs", report.history.window));
    kv_line!("Stability", &format!("{}%", report.history.stability));
    kv_line!("Trend",     &report.history.trend);
    y -= line_height_sm;

    {
        let layer = current_layer!();
        layer.set_fill_color(Color::Rgb(Rgb::new(0.35, 0.35, 0.35, None)));
        layer.use_text(
            &format!("{:<22} {:>12} {:>10} {:>10} {:>8}",
                "Timestamp", "Confidence", "Readiness", "Duration", "Risk"),
            7.5,
            left_margin + Mm(2.0),
            y,
            &font,
        );
        y -= line_height_sm;
    }

    for run in &report.history.runs {
        let ts = if run.timestamp.len() >= 19 {
            &run.timestamp[..19]
        } else {
            &run.timestamp
        };
        mono_line!(&format!(
            "{:<22} {:>11}% {:>9}% {:>9}s {:>8}",
            ts.replace('T', " "),
            run.confidence,
            run.readiness.unwrap_or(0),
            run.duration_seconds,
            run.risk,
        ));
    }

    // ══════════════════════════════════════════════
    // 3. BASELINE CONTRACT
    // ══════════════════════════════════════════════
    section_heading!("3. Baseline Contract");
    kv_line!("Pinned",  if report.baseline.pinned { "Yes" } else { "No" });
    kv_line!("Status",  &report.baseline.status);

    if report.baseline.pinned {
        if let Some(c) = report.baseline.expected_confidence {
            kv_line!("Expected Confidence", &format!("{}%", c));
        }
        if let Some(r) = report.baseline.expected_readiness {
            kv_line!("Expected Readiness", &format!("{}%", r));
        }
        if let Some(d) = report.baseline.expected_duration {
            kv_line!("Expected Duration", &format!("{}s", d));
        }
        if let Some(delta) = report.baseline.confidence_delta {
            kv_line!("Confidence Δ", &format!("{:+}%", delta));
        }
        if let Some(delta) = report.baseline.readiness_delta {
            kv_line!("Readiness Δ",  &format!("{:+}%", delta));
        }
        if let Some(delta) = report.baseline.duration_delta_pct {
            kv_line!("Duration Δ",   &format!("{:+}%", delta));
        }
        if !report.baseline.new_services.is_empty() {
            kv_line!("New Services",     &report.baseline.new_services.join(", "));
        }
        if !report.baseline.missing_services.is_empty() {
            kv_line!("Missing Services", &report.baseline.missing_services.join(", "));
        }
    }

    // ══════════════════════════════════════════════
    // 4. POLICY COMPLIANCE
    // ══════════════════════════════════════════════
    section_heading!("4. Policy Compliance");
    kv_line!("Configured", if report.policy.configured { "Yes" } else { "No" });
    kv_line!("Verdict",    &report.policy.verdict);

    if !report.policy.checks.is_empty() {
        y -= line_height_sm;
        {
            let layer = current_layer!();
            layer.set_fill_color(Color::Rgb(Rgb::new(0.35, 0.35, 0.35, None)));
            layer.use_text(
                &format!("{:<30} {:>8} {:>8}  {}", "Rule", "Setting", "Result", "Detail"),
                7.5,
                left_margin + Mm(2.0),
                y,
                &font,
            );
            y -= line_height_sm;
        }

        for check in &report.policy.checks {
            mono_line!(&format!(
                "{:<30} {:>8} {:>8}  {}",
                check.rule, check.setting, check.result, check.detail
            ));
        }
    }

    // ══════════════════════════════════════════════
    // 5. PREFLIGHT
    // ══════════════════════════════════════════════
    section_heading!("5. Preflight");
    kv_line!("Restore Readiness Score", &format!("{}%", report.preflight.restore_readiness_score));

    for f in &report.preflight.findings {
        mono_line!(&format!("[{}] {}", f.severity, f.message));
    }

    // ══════════════════════════════════════════════
    // 6. PROVIDER
    // ══════════════════════════════════════════════
    section_heading!("6. Backup Provider");
    kv_line!("Attached", if report.provider.attached { "Yes" } else { "No" });

    if report.provider.attached {
        if let Some(ref n) = report.provider.name {
            kv_line!("Name", n);
        }
        if let Some(ref k) = report.provider.kind {
            kv_line!("Kind", k);
        }
        if let Some(ref r) = report.provider.repository {
            kv_line!("Repository", r);
        }
        match report.provider.verified {
            Some(true)  => kv_line!("Verified", "YES — OK"),
            Some(false) => kv_line!("Verified", "NO — FAILED"),
            None        => kv_line!("Verified", "Not checked at report time — run `rehearsa provider verify`"),
        }
    }

    // ══════════════════════════════════════════════
    // FOOTER
    // ══════════════════════════════════════════════
    {
        let layer = current_layer!();
        let footer_y = Mm(12.0);
        draw_rule(&layer, footer_y);
        layer.set_fill_color(Color::Rgb(Rgb::new(0.55, 0.55, 0.55, None)));
        layer.use_text(
            &format!(
                "Rehearsa v{}  |  Report ID: {}  |  {}",
                report.meta.rehearsa_version,
                report.meta.report_id,
                &report.meta.generated_at[..10],
            ),
            7.0,
            left_margin,
            Mm(8.0),
            &font_regular,
        );
    }

    // ──────────────────────────────────────────────
    // Serialise to bytes
    // ──────────────────────────────────────────────
    let mut buf = std::io::BufWriter::new(std::io::Cursor::new(Vec::new()));
    doc.save(&mut buf).map_err(|e| format!("PDF save error: {}", e))?;

    let cursor = buf.into_inner().map_err(|e| format!("PDF flush error: {}", e))?;
    Ok(cursor.into_inner())
}

// ======================================================
// HELPERS
// ======================================================

/// ASCII mini bar for service score column.
fn score_bar(score: u32) -> String {
    let filled = (score / 10) as usize;
    let empty  = 10 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Simple UUID-shaped hex string without the uuid crate dependency.
fn uuid_v4_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    // Mix with a stack address for uniqueness within a process
    let addr = &nanos as *const u32 as u64;

    format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        nanos,
        (addr >> 32) & 0xffff,
        (addr >> 16) & 0x0fff,
        (addr & 0x3fff) | 0x8000,
        addr & 0xffffffffffff,
    )
}

// ======================================================
// ENTRY POINTS (called from main.rs)
// ======================================================

pub struct ReportArgs {
    pub stack:    Option<String>,   // None = fleet
    pub format:   ReportFormat,
    pub output:   Option<String>,   // None = stdout / current dir
    pub provider: Option<String>,
    pub window:   usize,
}

#[derive(Clone, PartialEq)]
pub enum ReportFormat {
    Json,
    Pdf,
    Both,
}

pub fn run_report(args: &ReportArgs) -> Result<(), String> {
    match args.stack {
        Some(ref stack) => run_single_report(stack, args),
        None            => run_fleet_report(args),
    }
}

fn run_single_report(stack: &str, args: &ReportArgs) -> Result<(), String> {
    let opts = ReportOptions {
        stack:          stack.to_string(),
        history_window: args.window,
        provider_name:  args.provider.clone(),
    };

    let report = build_report(&opts)?;

    if args.format == ReportFormat::Json || args.format == ReportFormat::Both {
        let json = render_json(&report)?;
        let path = resolve_output_path(&args.output, stack, "json");
        write_or_print(&json.into_bytes(), &path, "json")?;
    }

    if args.format == ReportFormat::Pdf || args.format == ReportFormat::Both {
        let pdf = render_pdf(&report)?;
        let path = resolve_output_path(&args.output, stack, "pdf");
        write_or_print(&pdf, &path, "pdf")?;
    }

    Ok(())
}

fn run_fleet_report(args: &ReportArgs) -> Result<(), String> {
    let reports = build_fleet_report();

    if reports.is_empty() {
        return Err("No stacks with rehearsal history found.".to_string());
    }

    // JSON fleet: one file, array of all reports
    if args.format == ReportFormat::Json || args.format == ReportFormat::Both {
        let json = render_json_fleet(&reports)?;
        let path = resolve_output_path(&args.output, "fleet", "json");
        write_or_print(&json.into_bytes(), &path, "json")?;
    }

    // PDF fleet: one PDF per stack (PDF is a per-stack visual document)
    if args.format == ReportFormat::Pdf || args.format == ReportFormat::Both {
        for report in &reports {
            let pdf = render_pdf(report)?;
            let path = resolve_output_path(&args.output, &report.meta.target, "pdf");
            write_or_print(&pdf, &path, "pdf")?;
        }
    }

    Ok(())
}

fn resolve_output_path(
    output_arg: &Option<String>,
    stem: &str,
    ext: &str,
) -> String {
    match output_arg {
        Some(path) if path.ends_with('/') || std::path::Path::new(path).is_dir() => {
            format!("{}{}-report.{}", path, stem, ext)
        }
        Some(path) if ext == "json" || path.ends_with(&format!(".{}", ext)) => {
            path.clone()
        }
        Some(path) => path.clone(),
        None if ext == "pdf" => format!("{}-report.pdf", stem),
        None => "-".to_string(), // stdout for JSON
    }
}

fn write_or_print(bytes: &[u8], path: &str, kind: &str) -> Result<(), String> {
    if path == "-" {
        // stdout — only valid for JSON
        let s = std::str::from_utf8(bytes)
            .map_err(|e| format!("UTF-8 error: {}", e))?;
        print!("{}", s);
        return Ok(());
    }

    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create output directory: {}", e))?;
        }
    }

    fs::write(path, bytes)
        .map_err(|e| format!("Failed to write {}: {}", path, e))?;

    println!("Report written: {} ({})", path, kind.to_uppercase());
    Ok(())
}

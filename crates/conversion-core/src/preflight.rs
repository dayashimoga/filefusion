//! Pre-flight unsupported feature warnings (`FR-FID-002`) and structural fidelity
//! analyzers (`FR-FID-001`) for document and media conversion pipelines.

use file_core::{
    CORE_SCHEMA_VERSION, ConversionFidelityReport, ConversionRoute, FidelitySeverity,
    FileInspectionReport, FormatId, PreflightAssessment, PreflightWarning, QualityMetrics,
};
use serde::{Deserialize, Serialize};

use crate::graph::ResolvedConversionPlan;

/// Quantitative structural inventory extracted from markup or document text (`FR-FID-001`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuralInventory {
    /// Count of heading elements (`<h1>..<h6>`, `# ` ATX headings).
    pub headings: u32,
    /// Count of tables (`<table>` or Markdown pipe header separator rows).
    pub tables: u32,
    /// Count of hyperlinks (`<a href=` or Markdown `[label](url)`).
    pub links: u32,
    /// Count of embedded image references (`<img ` or Markdown `![alt](url)`).
    pub images: u32,
}

/// Evaluates conversion compatibility and emits structured [`PreflightWarning`] items prior to
/// running a conversion job (`FR-FID-002`).
#[must_use]
pub fn evaluate_preflight(
    inspection: &FileInspectionReport,
    target: FormatId,
    plan: Option<&ResolvedConversionPlan>,
) -> PreflightAssessment {
    let source = inspection.detected_format;
    let warnings = collect_preflight_warnings(inspection, source, target, plan);

    let primary_route: Option<ConversionRoute> = plan.map(|p| {
        if p.hops.len() == 1 {
            p.hops[0].clone()
        } else {
            ConversionRoute {
                from: p.source,
                to: p.target,
                is_lossless_direct: p.is_lossless_chain,
                requires_tier2_tool: p.required_tier2_tools.first().cloned(),
            }
        }
    });

    let est_mb = inspection
        .byte_size
        .div_ceil(1_048_576)
        .saturating_mul(4)
        .clamp(16, 1024);

    PreflightAssessment {
        supported: plan.is_some() && !inspection.metadata_summary.is_encrypted,
        route: primary_route,
        warnings,
        estimated_memory_mb: est_mb,
    }
}

fn push_warning(
    warnings: &mut Vec<PreflightWarning>,
    code: &str,
    severity: FidelitySeverity,
    message: String,
) {
    warnings.push(PreflightWarning {
        code: String::from(code),
        severity,
        message,
    });
}

fn collect_preflight_warnings(
    inspection: &FileInspectionReport,
    source: FormatId,
    target: FormatId,
    plan: Option<&ResolvedConversionPlan>,
) -> Vec<PreflightWarning> {
    let mut warnings = Vec::new();

    if inspection.metadata_summary.is_encrypted {
        push_warning(
            &mut warnings,
            "FID-DRM-ENCRYPTED",
            FidelitySeverity::BreakingLoss,
            String::from(
                "Source container appears encrypted/DRM-protected; unlock before converting.",
            ),
        );
    }

    if matches!(source, FormatId::Xlsx | FormatId::Ods)
        && matches!(target, FormatId::Csv | FormatId::Json | FormatId::PlainText)
    {
        push_warning(
            &mut warnings,
            "FID-XLSX-FORMULAS",
            FidelitySeverity::BreakingLoss,
            format!(
                "Spreadsheet formulas, multiple sheets, and styling are flattened when exporting {source:?} to {target:?}."
            ),
        );
    }

    if matches!(source, FormatId::Pptx | FormatId::Odp)
        && matches!(target, FormatId::Pdf | FormatId::Png | FormatId::Jpeg)
    {
        push_warning(
            &mut warnings,
            "FID-PPTX-ANIMATIONS",
            FidelitySeverity::BreakingLoss,
            format!(
                "Slide transitions, animations, and embedded speaker timings are dropped in static {target:?} output."
            ),
        );
    }

    if matches!(source, FormatId::Markdown | FormatId::Html) && target == FormatId::PlainText {
        push_warning(
            &mut warnings,
            "FID-MARKUP-PLAINTEXT",
            FidelitySeverity::Warning,
            format!(
                "Headings, table grid formatting, hyperlinks, and styling from {source:?} are stripped in PlainText."
            ),
        );
    }

    if matches!(source, FormatId::Json | FormatId::Yaml | FormatId::Xml) && target == FormatId::Csv
    {
        push_warning(
            &mut warnings,
            "FID-CSV-FLATTEN",
            FidelitySeverity::Warning,
            format!(
                "Hierarchical objects and arrays from {source:?} are flattened into 2D CSV cells."
            ),
        );
    }

    collect_image_and_plan_warnings(&mut warnings, inspection, source, target, plan);
    warnings
}

fn collect_image_and_plan_warnings(
    warnings: &mut Vec<PreflightWarning>,
    inspection: &FileInspectionReport,
    source: FormatId,
    target: FormatId,
    plan: Option<&ResolvedConversionPlan>,
) {
    let source_has_alpha = inspection
        .metadata_summary
        .color_space
        .as_deref()
        .is_some_and(|cs| {
            let low = cs.to_ascii_lowercase();
            low.contains("rgba") || low.contains("alpha")
        })
        || matches!(source, FormatId::Png | FormatId::Webp | FormatId::Gif);
    if source_has_alpha && matches!(target, FormatId::Jpeg | FormatId::Bmp) {
        push_warning(
            warnings,
            "FID-ALPHA-DROP",
            FidelitySeverity::Warning,
            format!(
                "Transparency/alpha channels cannot be stored in {target:?} and will be flattened."
            ),
        );
    }

    if source == FormatId::Gif
        && matches!(
            target,
            FormatId::Png | FormatId::Jpeg | FormatId::Bmp | FormatId::Tiff
        )
    {
        push_warning(
            warnings,
            "FID-ANIM-FLATTEN",
            FidelitySeverity::Warning,
            format!("Multi-frame GIF animation flattens to first frame in {target:?}."),
        );
    }

    if let Some(p) = plan {
        if !p.is_lossless_chain && warnings.is_empty() {
            push_warning(
                warnings,
                "FID-LOSSY-TRANSFORM",
                FidelitySeverity::Info,
                format!(
                    "Converting {source:?} -> {target:?} uses perceptual encoding or container re-encoding."
                ),
            );
        }
        if !p.required_tier2_tools.is_empty() {
            push_warning(
                warnings,
                "FID-TIER2-REQUIRED",
                FidelitySeverity::Info,
                format!(
                    "Route executes out-of-process via isolated Tier 2 binary: {}.",
                    p.required_tier2_tools.join(", ")
                ),
            );
        }
    }
}

/// Scans markup/document text (`Markdown` or `HTML`) to inventory headings, tables, hyperlinks,
/// and embedded image references (`FR-FID-001`).
#[must_use]
pub fn analyze_markup_structure(text: &str, format: FormatId) -> StructuralInventory {
    let mut inv = StructuralInventory::default();
    let lower = text.to_ascii_lowercase();

    match format {
        FormatId::Html => {
            for tag in ["<h1", "<h2", "<h3", "<h4", "<h5", "<h6"] {
                inv.headings = inv
                    .headings
                    .saturating_add(u32::try_from(lower.matches(tag).count()).unwrap_or(0));
            }
            inv.tables = u32::try_from(lower.matches("<table").count()).unwrap_or(0);
            inv.links = u32::try_from(lower.matches("<a ").count()).unwrap_or(0);
            inv.images = u32::try_from(lower.matches("<img ").count()).unwrap_or(0);
        }
        FormatId::Markdown => {
            for line in text.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') && trimmed.trim_start_matches('#').starts_with(' ') {
                    inv.headings = inv.headings.saturating_add(1);
                }
                if trimmed.starts_with('|') && trimmed.contains("---") {
                    inv.tables = inv.tables.saturating_add(1);
                }
            }
            let img_count = text.matches("![").count();
            let bracket_open = text.matches("](").count();
            inv.images = u32::try_from(img_count).unwrap_or(0);
            inv.links = u32::try_from(bracket_open.saturating_sub(img_count)).unwrap_or(0);
        }
        _ => {}
    }

    inv
}

/// Builds a normalized [`ConversionFidelityReport`] comparing optional pre/post structural inventories
/// and empirical [`QualityMetrics`] (`FR-FID-001`).
#[must_use]
pub fn build_fidelity_report(
    source_format: FormatId,
    target_format: FormatId,
    warnings: Vec<PreflightWarning>,
    before: Option<StructuralInventory>,
    after: Option<StructuralInventory>,
    quality: Option<QualityMetrics>,
) -> ConversionFidelityReport {
    let (headings_preserved, tables_preserved, links_preserved, images_preserved) =
        match (before, after) {
            (Some(b), Some(a)) => (
                Some(a.headings.min(b.headings)),
                Some(a.tables.min(b.tables)),
                Some(a.links.min(b.links)),
                Some(a.images.min(b.images)),
            ),
            _ => (None, None, None, None),
        };

    ConversionFidelityReport {
        schema_version: CORE_SCHEMA_VERSION,
        source_format,
        target_format,
        warnings,
        headings_preserved,
        tables_preserved,
        links_preserved,
        images_preserved,
        quality,
    }
}

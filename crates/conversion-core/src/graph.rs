//! Multi-hop conversion graph with direct-first and lossless-hub Dijkstra routing
//! (`FF-MOD-001` §3, `FF-CTR-001` §3.2).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use file_core::{ConversionRoute, ErrorCode, FileFusionError, FileFusionResult, FormatId};
use serde::{Deserialize, Serialize};

/// Resolved path across one or more [`ConversionRoute`] hops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedConversionPlan {
    /// Requested input format.
    pub source: FormatId,
    /// Requested target format.
    pub target: FormatId,
    /// Ordered sequence of directed hops (`len() == 1` when direct, `> 1` when routed via hub).
    pub hops: Vec<ConversionRoute>,
    /// Cumulative routing cost (lower favors direct lossless paths).
    pub total_cost: u32,
    /// Unique Tier 2 external tools required across all hops.
    pub required_tier2_tools: Vec<String>,
    /// `true` when every hop preserves data without lossy quantization or rasterization.
    pub is_lossless_chain: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct QueueNode {
    cost: u32,
    hops: u32,
    format: FormatId,
}

impl Ord for QueueNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| other.hops.cmp(&self.hops))
    }
}

impl PartialOrd for QueueNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Directed weighted format conversion graph supporting direct-first and lossless-hub selection.
#[derive(Debug, Clone)]
pub struct ConversionGraph {
    routes: Vec<ConversionRoute>,
}

impl Default for ConversionGraph {
    fn default() -> Self {
        Self::new_baseline()
    }
}

impl ConversionGraph {
    /// Constructs the baseline `FileFusion` conversion graph covering raster images, containers,
    /// audio/video streams, structured markup/data, office documents, and ebooks.
    #[must_use]
    pub fn new_baseline() -> Self {
        let mut graph = Self { routes: Vec::new() };
        graph.register_raster_routes();
        graph.register_archive_routes();
        graph.register_audio_and_video_routes();
        graph.register_document_and_text_routes();
        graph
    }

    /// Returns all registered directed conversion routes.
    #[must_use]
    pub fn routes(&self) -> &[ConversionRoute] {
        &self.routes
    }

    fn push_route(&mut self, from: FormatId, to: FormatId, lossless: bool, tier2: Option<&str>) {
        if from == to {
            return;
        }
        if self.routes.iter().any(|r| r.from == from && r.to == to) {
            return;
        }
        self.routes.push(ConversionRoute {
            from,
            to,
            is_lossless_direct: lossless,
            requires_tier2_tool: tier2.map(str::to_owned),
        });
    }

    fn register_raster_routes(&mut self) {
        let raster_formats = [
            FormatId::Png,
            FormatId::Jpeg,
            FormatId::Webp,
            FormatId::Avif,
            FormatId::Gif,
            FormatId::Bmp,
            FormatId::Tiff,
            FormatId::Ico,
        ];
        for &src in &raster_formats {
            for &dst in &raster_formats {
                if src == dst {
                    continue;
                }
                let lossless = matches!(
                    dst,
                    FormatId::Png | FormatId::Bmp | FormatId::Tiff | FormatId::Webp
                ) && matches!(
                    src,
                    FormatId::Png | FormatId::Bmp | FormatId::Tiff | FormatId::Gif
                );
                self.push_route(src, dst, lossless, None);
            }
        }
    }

    fn register_archive_routes(&mut self) {
        let archives = [
            FormatId::Zip,
            FormatId::Tar,
            FormatId::Gzip,
            FormatId::Zstd,
            FormatId::Xz,
            FormatId::Bzip2,
            FormatId::Lz4,
            FormatId::SevenZ,
        ];
        for &src in &archives {
            for &dst in &archives {
                self.push_route(src, dst, true, None);
            }
        }
    }

    fn register_audio_and_video_routes(&mut self) {
        // Tier 1 pure-Rust lossless & decode -> WAV/FLAC hub.
        for &src in &[
            FormatId::Wav,
            FormatId::Flac,
            FormatId::Mp3,
            FormatId::Ogg,
            FormatId::Aac,
            FormatId::M4a,
        ] {
            self.push_route(src, FormatId::Wav, src == FormatId::Flac, None);
            self.push_route(
                src,
                FormatId::Flac,
                matches!(src, FormatId::Wav | FormatId::Flac),
                None,
            );
        }

        // Tier 2 FFmpeg audio lossy targets.
        for &dst in &[
            FormatId::Mp3,
            FormatId::Ogg,
            FormatId::Opus,
            FormatId::Aac,
            FormatId::M4a,
        ] {
            for &hub in &[FormatId::Wav, FormatId::Flac] {
                self.push_route(hub, dst, false, Some("tier2-ffmpeg"));
            }
        }

        // Video container pairwise & audio-track extraction (`tier2-ffmpeg`).
        let videos = [
            FormatId::Mp4,
            FormatId::Mov,
            FormatId::Mkv,
            FormatId::Webm,
            FormatId::Avi,
        ];
        for &src in &videos {
            for &dst in &videos {
                let lossless_remux = matches!(
                    (src, dst),
                    (FormatId::Mp4, FormatId::Mov | FormatId::Mkv)
                        | (FormatId::Mov, FormatId::Mp4 | FormatId::Mkv)
                        | (FormatId::Mkv, FormatId::Mp4 | FormatId::Mov)
                );
                self.push_route(src, dst, lossless_remux, Some("tier2-ffmpeg"));
            }
            self.push_route(src, FormatId::Wav, false, Some("tier2-ffmpeg"));
            self.push_route(src, FormatId::Flac, false, Some("tier2-ffmpeg"));
            self.push_route(src, FormatId::Mp3, false, Some("tier2-ffmpeg"));
            self.push_route(src, FormatId::Opus, false, Some("tier2-ffmpeg"));
        }
    }

    fn register_document_and_text_routes(&mut self) {
        // Bidirectional structured text/markup (`Markdown`, `Html`, `PlainText`)
        for &src in &[FormatId::Markdown, FormatId::Html, FormatId::PlainText] {
            for &dst in &[FormatId::Markdown, FormatId::Html, FormatId::PlainText] {
                self.push_route(
                    src,
                    dst,
                    matches!(dst, FormatId::Html | FormatId::Markdown),
                    None,
                );
            }
        }

        // Bidirectional tabular & structured data (`Csv`, `Json`, `Yaml`, `Xml`)
        for &src in &[FormatId::Csv, FormatId::Json, FormatId::Yaml, FormatId::Xml] {
            for &dst in &[FormatId::Csv, FormatId::Json, FormatId::Yaml, FormatId::Xml] {
                let lossless = !matches!(dst, FormatId::Csv);
                self.push_route(src, dst, lossless, None);
            }
            for &markup_dst in &[FormatId::Markdown, FormatId::Html, FormatId::PlainText] {
                self.push_route(src, markup_dst, false, None);
            }
        }

        // Office -> PDF (`LibreOffice`) and Ebook -> EPUB/MOBI (`Calibre`/`Pandoc`)
        for &office in &[
            FormatId::Docx,
            FormatId::Xlsx,
            FormatId::Pptx,
            FormatId::Odt,
            FormatId::Ods,
            FormatId::Odp,
            FormatId::Rtf,
        ] {
            self.push_route(office, FormatId::Pdf, false, Some("tier2-libreoffice"));
        }
        for &text_fmt in &[FormatId::Markdown, FormatId::Html] {
            self.push_route(text_fmt, FormatId::Epub, true, None);
            self.push_route(text_fmt, FormatId::Docx, false, Some("tier2-pandoc"));
        }
        self.push_route(FormatId::Epub, FormatId::Mobi, false, Some("tier2-calibre"));
        self.push_route(FormatId::Mobi, FormatId::Epub, false, Some("tier2-calibre"));
    }

    /// Resolves the minimum-cost conversion path from `from` to `to`, preferring direct lossless
    /// routes over multi-hop lossless hubs (`PNG`, `WAV`, `HTML`, `JSON`) and optionally
    /// excluding Tier 2 external binary hops (`allow_tier2 == false` for strict WASM/offline runs).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`FmtUnsupported`) if no path satisfies `allow_tier2`.
    pub fn resolve_route_plan(
        &self,
        from: FormatId,
        to: FormatId,
        allow_tier2: bool,
    ) -> FileFusionResult<ResolvedConversionPlan> {
        if from == to {
            return Ok(ResolvedConversionPlan {
                source: from,
                target: to,
                hops: vec![ConversionRoute {
                    from,
                    to,
                    is_lossless_direct: true,
                    requires_tier2_tool: None,
                }],
                total_cost: 0,
                required_tier2_tools: Vec::new(),
                is_lossless_chain: true,
            });
        }

        let mut best_cost: HashMap<FormatId, u32> = HashMap::new();
        let mut prev_hop: HashMap<FormatId, ConversionRoute> = HashMap::new();
        let mut heap = BinaryHeap::new();

        best_cost.insert(from, 0);
        heap.push(QueueNode {
            cost: 0,
            hops: 0,
            format: from,
        });

        while let Some(QueueNode { cost, hops, format }) = heap.pop() {
            if format == to {
                break;
            }
            if best_cost.get(&format).is_some_and(|&c| cost > c) {
                continue;
            }

            for route in self.routes.iter().filter(|r| r.from == format) {
                if !allow_tier2 && route.requires_tier2_tool.is_some() {
                    continue;
                }
                let step_cost = edge_weight(route);
                let next_cost = cost.saturating_add(step_cost);
                let better = best_cost
                    .get(&route.to)
                    .is_none_or(|&prev| next_cost < prev);
                if better {
                    best_cost.insert(route.to, next_cost);
                    prev_hop.insert(route.to, route.clone());
                    heap.push(QueueNode {
                        cost: next_cost,
                        hops: hops.saturating_add(1),
                        format: route.to,
                    });
                }
            }
        }

        if !prev_hop.contains_key(&to) {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("no registered conversion route from {from:?} to {to:?}"),
            ));
        }

        let mut path_hops = Vec::new();
        let mut cursor = to;
        while cursor != from {
            let Some(hop) = prev_hop.get(&cursor).cloned() else {
                break;
            };
            cursor = hop.from;
            path_hops.push(hop);
        }
        path_hops.reverse();

        let total_cost = best_cost.get(&to).copied().unwrap_or(u32::MAX);
        let mut required_tier2_tools = Vec::new();
        let mut is_lossless_chain = true;
        for hop in &path_hops {
            if !hop.is_lossless_direct {
                is_lossless_chain = false;
            }
            if let Some(tool) = &hop.requires_tier2_tool {
                if !required_tier2_tools.contains(tool) {
                    required_tier2_tools.push(tool.clone());
                }
            }
        }

        Ok(ResolvedConversionPlan {
            source: from,
            target: to,
            hops: path_hops,
            total_cost,
            required_tier2_tools,
            is_lossless_chain,
        })
    }
}

fn edge_weight(route: &ConversionRoute) -> u32 {
    let base = if route.is_lossless_direct { 10 } else { 30 };
    let tier2_penalty = if route.requires_tier2_tool.is_some() {
        40
    } else {
        0
    };
    base + tier2_penalty
}

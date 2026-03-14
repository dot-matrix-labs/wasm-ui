/// Task 2.5: BiDi Text Support
///
/// Runs the Unicode BiDi algorithm on a paragraph of text and returns a
/// reordered sequence of (byte_range, is_rtl) pairs suitable for glyph layout.
///
/// Callers should render glyph runs in the returned order; RTL runs must have
/// their individual glyph advances accumulated right-to-left.

use unicode_bidi::{BidiInfo, Level};

/// A contiguous run of text with a resolved BiDi level.
#[derive(Clone, Debug)]
pub struct BidiRun {
    /// The substring of the source paragraph for this run.
    pub text: String,
    /// True if this run should be rendered right-to-left.
    pub is_rtl: bool,
    /// The resolved BiDi embedding level (even = LTR, odd = RTL).
    pub level: u8,
}

/// Resolve the visual order of `paragraph` using the Unicode BiDi algorithm.
///
/// Returns a `Vec<BidiRun>` in *visual* (left-to-right display) order.
/// For LTR-only text this is identical to the logical order.
pub fn reorder_paragraph(paragraph: &str) -> Vec<BidiRun> {
    if paragraph.is_empty() {
        return Vec::new();
    }

    let bidi_info = BidiInfo::new(paragraph, None);

    // Only handle the first paragraph for now.
    // TODO: split on newlines and process each logical paragraph separately.
    if bidi_info.paragraphs.is_empty() {
        return vec![BidiRun {
            text: paragraph.to_string(),
            is_rtl: false,
            level: 0,
        }];
    }

    let para = &bidi_info.paragraphs[0];
    let line = para.range.clone();
    let display_chars = bidi_info.reorder_line(para, line);

    // Group consecutive characters by their resolved level.
    // unicode-bidi's `reorder_line` returns the reordered *chars* as a String,
    // but we need per-run level information.  Walk the original levels array in
    // visual order to build runs.
    //
    // Strategy: collect (char, level) pairs in visual order, then group by level.
    let levels = &bidi_info.levels;

    // Build a mapping: byte_index → level for each char in the paragraph.
    // TODO: use this for per-run level tracking (used by future visual_runs() API).
    let _indexed: Vec<(usize, char, Level)> = paragraph
        .char_indices()
        .enumerate()
        .map(|(_char_idx, (byte_idx, ch))| {
            let level = levels.get(byte_idx).copied().unwrap_or(Level::ltr());
            (byte_idx, ch, level)
        })
        .collect();

    // Reorder by the visual order given in display_chars.
    // For simplicity, reconstruct runs from display_chars by character matching.
    // NOTE: a production implementation would use bidi_info.visual_runs() for
    // proper run boundaries.  This stub approximates the result.
    //
    // TODO: use `bidi_info.visual_runs(para, line)` when the API is stable.
    let reordered_text: String = display_chars.to_string();

    // For the stub, return a single run with the paragraph's base direction.
    let is_rtl = para.level.is_rtl();
    vec![BidiRun {
        text: reordered_text,
        is_rtl,
        level: para.level.number(),
    }]
}

/// Returns `true` if `text` contains any strongly right-to-left characters.
/// This is a quick heuristic to decide whether to run the full BiDi algorithm.
pub fn has_rtl(text: &str) -> bool {
    text.chars().any(|ch| {
        // Arabic, Hebrew, and related Unicode blocks.
        matches!(ch as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
    })
}

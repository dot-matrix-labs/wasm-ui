/// Task 2.3: Proportional Text Metrics
///
/// Trait for querying per-glyph advance widths.  The default implementation
/// in this module uses the monospace `font_size * 0.6` approximation.  The
/// WASM layer wires up the real `TextAtlas` implementation so that `Ui` can
/// use actual glyph advances without depending on `fontdue` directly.

pub trait GlyphMetrics {
    /// Return the advance width of `ch` at the given `font_size` in pixels.
    fn advance(&self, ch: char, font_size: f32) -> f32;

    /// Compute per-grapheme advance prefix sums for `text` at `font_size`.
    ///
    /// `prefix[i]` is the x-offset of grapheme `i`; `prefix[n]` is the total
    /// line width.  The default implementation calls `self.advance` for the
    /// first `char` of each grapheme cluster.
    fn advance_prefix_sums(&self, text: &str, font_size: f32) -> Vec<f32> {
        use unicode_segmentation::UnicodeSegmentation;
        let mut sums = vec![0.0f32];
        let mut acc = 0.0f32;
        for grapheme in text.graphemes(true) {
            let ch = grapheme.chars().next().unwrap_or(' ');
            acc += self.advance(ch, font_size);
            sums.push(acc);
        }
        sums
    }

    /// Binary-search the prefix-sum array to find the grapheme index closest
    /// to pixel offset `x` within a line.  Returns the grapheme index (0-based).
    fn index_for_x(&self, prefix: &[f32], x: f32) -> usize {
        if prefix.len() <= 1 {
            return 0;
        }
        // prefix[0] == 0, prefix[n] == total width.
        // Find i such that prefix[i] <= x < prefix[i+1], choosing the closer.
        let n = prefix.len() - 1; // number of graphemes
        match prefix.binary_search_by(|p| p.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Less)) {
            Ok(i) => i.min(n),
            Err(i) => {
                // x is between prefix[i-1] and prefix[i]
                if i == 0 {
                    return 0;
                }
                if i > n {
                    return n;
                }
                // Snap to the closer boundary.
                let left = prefix[i - 1];
                let right = prefix[i];
                if (x - left) <= (right - x) {
                    i - 1
                } else {
                    i
                }
            }
        }
    }
}

/// Default monospace approximation — no atlas needed.
pub struct MonospaceMetrics;

impl GlyphMetrics for MonospaceMetrics {
    fn advance(&self, _ch: char, font_size: f32) -> f32 {
        font_size * 0.6
    }
}

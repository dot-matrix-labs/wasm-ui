/// A renderer-agnostic callback that returns the advance width of a single
/// Unicode character at the given font size (in logical pixels).
///
/// `ui-core` never links against `fontdue` directly — instead the WASM layer
/// builds a `TextMeasure` from the glyph atlas and passes it downward at
/// render/hit-test time.  Native tests can supply a monospace approximation.
///
/// # Implementing a real measure
/// ```ignore
/// let measure = TextMeasure::new({
///     let atlas = atlas.clone();
///     let font_size = 15.0;
///     move |ch: char| atlas.borrow_mut().ensure_glyph(ch, font_size).advance
/// });
/// ```
pub struct TextMeasure {
    // Heap-allocated so it can capture atlas state without lifetime parameters.
    f: Box<dyn Fn(char) -> f32 + Send + Sync>,
}

impl TextMeasure {
    /// Construct from any function / closure.
    pub fn new(f: impl Fn(char) -> f32 + Send + Sync + 'static) -> Self {
        Self { f: Box::new(f) }
    }

    /// Monospace fallback — use when no real font metrics are available.
    /// `char_width` is typically `font_size * 0.6`.
    pub fn monospace(char_width: f32) -> Self {
        Self::new(move |_ch| char_width)
    }

    /// Return the advance width of `ch`.
    #[inline]
    pub fn advance(&self, ch: char) -> f32 {
        (self.f)(ch)
    }

    /// Sum the advance widths of all characters in `text`.
    pub fn measure_str(&self, text: &str) -> f32 {
        text.chars().map(|ch| self.advance(ch)).sum()
    }

    /// Return a `Vec` of cumulative X offsets for each grapheme boundary in
    /// `text`, starting with `0.0`.  Length is `grapheme_count + 1`.
    ///
    /// Used by `position_to_index` to binary-search the clicked X position.
    pub fn cumulative_advances(&self, text: &str) -> Vec<f32> {
        use unicode_segmentation::UnicodeSegmentation;
        let mut advances = Vec::new();
        let mut x = 0.0f32;
        advances.push(x);
        for grapheme in text.graphemes(true) {
            // Sum advance widths of all scalar values in the grapheme cluster.
            for ch in grapheme.chars() {
                x += self.advance(ch);
            }
            advances.push(x);
        }
        advances
    }

    /// Map a pixel X offset (relative to the text origin) to the nearest
    /// grapheme boundary index, using the "click in the left half → previous
    /// boundary, right half → next boundary" rule.
    pub fn x_to_grapheme_index(&self, text: &str, x: f32) -> usize {
        let advances = self.cumulative_advances(text);
        if advances.len() <= 1 {
            return 0;
        }
        let n = advances.len() - 1; // number of graphemes
        // Binary search for the segment whose midpoint is closest to x.
        let mut lo = 0usize;
        let mut hi = n;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let mid_x = (advances[mid] + advances[mid + 1]) * 0.5;
            if x < mid_x {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        lo.min(n)
    }
}

impl std::fmt::Debug for TextMeasure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextMeasure(<fn>)")
    }
}

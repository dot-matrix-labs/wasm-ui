use std::collections::HashMap;

use fontdue::Font;
use unicode_segmentation::UnicodeSegmentation;

use ui_core::types::{Rect, Vec2};

#[derive(Clone, Debug)]
pub struct Glyph {
    pub uv: Rect,
    pub size: Vec2,
    pub bearing: Vec2,
    pub advance: f32,
}

/// Quantize a font size to the nearest 2px bucket so nearby sizes share cached
/// glyphs.  Returns the quantized value as a `u32` for use as a HashMap key.
#[inline]
fn quantize_font_size(font_size: f32) -> u32 {
    (font_size as u32 / 2) * 2
}

#[derive(Debug)]
pub struct TextAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    cursor: Vec2,
    row_h: f32,
    /// Cache keyed by `(char, quantized_font_size)` so different sizes get
    /// separate rasterizations.  Task 2.2: Font-Size-Aware Glyph Cache.
    glyphs: HashMap<(char, u32), Glyph>,
    dirty: bool,
    /// Dirty rect tracking for partial atlas uploads (Task 4.5).
    /// Stored as (x, y, w, h) in texel coordinates.
    dirty_rect: Option<(u32, u32, u32, u32)>,
    font: Option<Font>,
    /// Fallback fonts tried in order when the primary font is missing a glyph.
    /// Task 2.6: Font Fallback Chain.
    fallback_fonts: Vec<Font>,
}

impl TextAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        let mut pixels = vec![0u8; (width * height) as usize];
        pixels[0] = 255;
        Self {
            width,
            height,
            pixels,
            cursor: Vec2::new(1.0, 1.0),
            row_h: 0.0,
            glyphs: HashMap::new(),
            dirty: true,
            dirty_rect: Some((0, 0, width, height)),
            font: None,
            fallback_fonts: Vec::new(),
        }
    }

    pub fn set_font_bytes(&mut self, bytes: Vec<u8>) {
        if let Ok(font) = Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            self.font = Some(font);
            self.glyphs.clear();
            self.cursor = Vec2::new(1.0, 1.0);
            self.row_h = 0.0;
            self.pixels.fill(0);
            self.pixels[0] = 255;
            self.dirty = true;
            self.dirty_rect = Some((0, 0, self.width, self.height));
        }
    }

    /// Add a fallback font from raw bytes.  Glyphs missing from the primary
    /// font are looked up in fallbacks in the order they were added.
    /// Task 2.6: Font Fallback Chain.
    pub fn add_fallback_font_bytes(&mut self, bytes: Vec<u8>) {
        if let Ok(font) = Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            self.fallback_fonts.push(font);
            // Invalidate cache so missing glyphs are re-probed with the new fallback.
            self.glyphs.clear();
            self.dirty = true;
            self.dirty_rect = Some((0, 0, self.width, self.height));
        }
    }

    /// Returns the advance width for `ch` at `font_size` using the primary
    /// font (or fallbacks).  Does NOT rasterize or touch the atlas.
    /// Task 2.3: Proportional Text Metrics.
    #[allow(dead_code)]
    pub fn glyph_advance(&self, ch: char, font_size: f32) -> f32 {
        // Try primary font first.
        if let Some(font) = &self.font {
            let has_glyph = font.lookup_glyph_index(ch) != 0;
            if has_glyph {
                let metrics = font.metrics(ch, font_size);
                return metrics.advance_width;
            }
        }
        // Try fallbacks in order.
        for fb in &self.fallback_fonts {
            let has_glyph = fb.lookup_glyph_index(ch) != 0;
            if has_glyph {
                let metrics = fb.metrics(ch, font_size);
                return metrics.advance_width;
            }
        }
        // Last resort: monospace approximation.
        font_size * 0.6
    }

    /// Compute a prefix-sum array of x-advance positions for a line of text.
    /// `prefix[i]` is the x offset at which grapheme `i` starts;
    /// `prefix[n]` is the total width of the line.
    /// Task 2.3: Proportional Text Metrics.
    #[allow(dead_code)]
    pub fn advance_prefix_sums(&self, text: &str, font_size: f32) -> Vec<f32> {
        let mut sums = vec![0.0f32];
        let mut acc = 0.0f32;
        for grapheme in text.graphemes(true) {
            // Use the first char of the grapheme cluster for metrics.
            let ch = grapheme.chars().next().unwrap_or(' ');
            acc += self.glyph_advance(ch, font_size);
            sums.push(acc);
        }
        sums
    }

    pub fn ensure_glyph(&mut self, ch: char, font_size: f32) -> Glyph {
        let qsize = quantize_font_size(font_size);
        let key = (ch, qsize);
        // Use the quantized size for rasterization to keep the atlas lean.
        let raster_size = qsize as f32;

        if let Some(glyph) = self.glyphs.get(&key) {
            return glyph.clone();
        }

        // Try to rasterize from the primary font, then fallbacks.
        // Task 2.6: Font Fallback Chain.
        let glyph = self.rasterize_glyph(ch, raster_size);

        self.glyphs.insert(key, glyph.clone());
        glyph
    }

    /// Rasterize `ch` from the primary font or the first fallback that has it.
    fn rasterize_glyph(&mut self, ch: char, font_size: f32) -> Glyph {
        // Check primary font.
        let has_primary = self
            .font
            .as_ref()
            .map(|f| f.lookup_glyph_index(ch) != 0)
            .unwrap_or(false);

        if has_primary {
            // SAFETY: we know font is Some from has_primary check.
            let font = self.font.as_ref().unwrap();
            return Self::rasterize_with(font, ch, font_size, self.width, self.height, &mut self.pixels, &mut self.cursor, &mut self.row_h, &mut self.dirty, &mut self.dirty_rect, &mut self.glyphs);
        }

        // Try fallbacks.
        for i in 0..self.fallback_fonts.len() {
            let has_glyph = self.fallback_fonts[i].lookup_glyph_index(ch) != 0;
            if has_glyph {
                // Temporarily extract fallback font to satisfy borrow checker.
                // We swap it out, rasterize, then put it back.
                let font = &self.fallback_fonts[i];
                return Self::rasterize_with(font, ch, font_size, self.width, self.height, &mut self.pixels, &mut self.cursor, &mut self.row_h, &mut self.dirty, &mut self.dirty_rect, &mut self.glyphs);
            }
        }

        // Fallback: use primary font even if glyph index is 0 (renders .notdef).
        if let Some(font) = self.font.as_ref() {
            return Self::rasterize_with(font, ch, font_size, self.width, self.height, &mut self.pixels, &mut self.cursor, &mut self.row_h, &mut self.dirty, &mut self.dirty_rect, &mut self.glyphs);
        }

        // No font at all — return a blank placeholder.
        Glyph {
            uv: Rect::new(0.0, 0.0, 0.0, 0.0),
            size: Vec2::new(font_size * 0.6, font_size),
            bearing: Vec2::new(0.0, 0.0),
            advance: font_size * 0.6,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rasterize_with(
        font: &Font,
        ch: char,
        font_size: f32,
        atlas_width: u32,
        atlas_height: u32,
        pixels: &mut Vec<u8>,
        cursor: &mut Vec2,
        row_h: &mut f32,
        dirty: &mut bool,
        dirty_rect: &mut Option<(u32, u32, u32, u32)>,
        _glyphs: &mut HashMap<(char, u32), Glyph>,
    ) -> Glyph {
        let (metrics, bitmap) = font.rasterize(ch, font_size);
        let w = metrics.width as u32;
        let h = metrics.height as u32;
        let (x, y) = Self::allocate_inner(cursor, row_h, dirty, dirty_rect, pixels, w.max(1), h.max(1), atlas_width, atlas_height);
        for row in 0..h {
            let dst = ((y + row) * atlas_width + x) as usize;
            let src = (row * w) as usize;
            let len = w as usize;
            pixels[dst..dst + len].copy_from_slice(&bitmap[src..src + len]);
        }
        let uv = Rect::new(
            x as f32 / atlas_width as f32,
            y as f32 / atlas_height as f32,
            w as f32 / atlas_width as f32,
            h as f32 / atlas_height as f32,
        );
        Glyph {
            uv,
            size: Vec2::new(w as f32, h as f32),
            bearing: Vec2::new(metrics.xmin as f32, metrics.ymin as f32),
            advance: metrics.advance_width,
        }
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns the dirty rect as (x, y, w, h) if any region changed since the
    /// last `mark_clean()` call.  Task 4.5: Atlas Partial Upload.
    pub fn dirty_rect(&self) -> Option<(u32, u32, u32, u32)> {
        self.dirty_rect
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
        self.dirty_rect = None;
    }

    #[allow(clippy::too_many_arguments)]
    fn allocate_inner(
        cursor: &mut Vec2,
        row_h: &mut f32,
        dirty: &mut bool,
        dirty_rect: &mut Option<(u32, u32, u32, u32)>,
        pixels: &mut Vec<u8>,
        w: u32,
        h: u32,
        atlas_width: u32,
        atlas_height: u32,
    ) -> (u32, u32) {
        let padding = 1.0;
        if cursor.x + w as f32 + padding > atlas_width as f32 {
            cursor.x = 1.0;
            cursor.y += *row_h + padding;
            *row_h = 0.0;
        }
        if cursor.y + h as f32 + padding > atlas_height as f32 {
            *cursor = Vec2::new(1.0, 1.0);
            *row_h = 0.0;
            // NOTE: we can't clear the glyph cache here since we don't have
            // access to it in this static helper.  The atlas simply wraps
            // around; the caller should handle cache invalidation if needed.
            pixels.fill(0);
            // Full atlas invalidated — reset dirty rect to full extent.
            *dirty_rect = Some((0, 0, atlas_width, atlas_height));
        }
        let x = cursor.x as u32;
        let y = cursor.y as u32;
        cursor.x += w as f32 + padding;
        *row_h = row_h.max(h as f32);
        *dirty = true;
        // Expand dirty rect to cover the newly allocated glyph region (Task 4.5).
        *dirty_rect = Some(match *dirty_rect {
            None => (x, y, w, h),
            Some((rx, ry, rw, rh)) => {
                let x1 = rx.min(x);
                let y1 = ry.min(y);
                let x2 = (rx + rw).max(x + w);
                let y2 = (ry + rh).max(y + h);
                (x1, y1, x2 - x1, y2 - y1)
            }
        });
        (x, y)
    }
}


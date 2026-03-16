use ui_core::icon::IconPack;

/// A CPU-side RGBA8 pixel buffer for icon sprite sheets, paired with an
/// `IconPack` that maps icon names to UV coordinates.
///
/// This is the browser-side counterpart to `TextAtlas` but uses RGBA format
/// (4 bytes per pixel) instead of R8. Icons are loaded once at init from a
/// pre-built sprite sheet, so there is no dynamic packing.
#[derive(Debug)]
pub struct IconAtlas {
    width: u32,
    height: u32,
    /// RGBA8 pixel data (4 bytes per pixel).
    pixels: Vec<u8>,
    dirty: bool,
    pack: IconPack,
}

impl IconAtlas {
    /// Create a new empty icon atlas.
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            pixels: Vec::new(),
            dirty: false,
            pack: IconPack::default(),
        }
    }

    /// Load icon data from raw RGBA pixels and a metadata manifest.
    ///
    /// `rgba_pixels` must contain exactly `width * height * 4` bytes.
    pub fn load(&mut self, rgba_pixels: Vec<u8>, width: u32, height: u32, pack: IconPack) {
        assert_eq!(
            rgba_pixels.len(),
            (width * height * 4) as usize,
            "pixel data size mismatch: expected {}x{}x4={}, got {}",
            width,
            height,
            width * height * 4,
            rgba_pixels.len()
        );
        self.width = width;
        self.height = height;
        self.pixels = rgba_pixels;
        self.pack = pack;
        self.dirty = true;
    }

    /// Returns the RGBA pixel data.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Returns the atlas texture width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the atlas texture height.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns `true` if the pixel data has changed since the last
    /// `mark_clean()` call and needs to be re-uploaded to the GPU.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the atlas as clean (pixel data has been uploaded to the GPU).
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Mark the atlas as needing re-upload (e.g. after context loss).
    pub fn invalidate_gpu_cache(&mut self) {
        if !self.pixels.is_empty() {
            self.dirty = true;
        }
    }

    /// Returns a reference to the loaded icon pack.
    pub fn pack(&self) -> &IconPack {
        &self.pack
    }

    /// Returns `true` if icon data has been loaded.
    pub fn is_loaded(&self) -> bool {
        !self.pixels.is_empty()
    }
}

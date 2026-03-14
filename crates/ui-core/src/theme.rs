use crate::types::Color;

#[derive(Clone, Debug)]
pub struct Theme {
    pub font_scale: f32,
    pub high_contrast: bool,
    pub colors: ThemeColors,
}

#[derive(Clone, Debug)]
pub struct ThemeColors {
    pub background: Color,
    pub surface: Color,
    pub text: Color,
    pub text_muted: Color,
    pub primary: Color,
    pub error: Color,
    pub success: Color,
    pub focus_ring: Color,
}

impl Theme {
    pub fn default_light() -> Self {
        Self {
            font_scale: 1.0,
            high_contrast: false,
            colors: ThemeColors {
                background: Color::rgba(0.97, 0.97, 0.96, 1.0),
                surface: Color::rgba(1.0, 1.0, 1.0, 1.0),
                text: Color::rgba(0.1, 0.1, 0.12, 1.0),
                text_muted: Color::rgba(0.4, 0.4, 0.45, 1.0),
                primary: Color::rgba(0.2, 0.45, 0.9, 1.0),
                error: Color::rgba(0.88, 0.2, 0.2, 1.0),
                success: Color::rgba(0.2, 0.7, 0.3, 1.0),
                // #0066cc — WCAG-compliant blue focus ring
                focus_ring: Color::rgba(0.0, 0.4, 0.8, 1.0),
            },
        }
    }

    /// Dark mode theme (~#1e1e1e background, #e0e0e0 text).
    pub fn default_dark() -> Self {
        Self {
            font_scale: 1.0,
            high_contrast: false,
            colors: ThemeColors {
                background: Color::rgba(0.118, 0.118, 0.118, 1.0), // #1e1e1e
                surface: Color::rgba(0.18, 0.18, 0.18, 1.0),       // #2e2e2e
                text: Color::rgba(0.878, 0.878, 0.878, 1.0),        // #e0e0e0
                text_muted: Color::rgba(0.6, 0.6, 0.6, 1.0),
                primary: Color::rgba(0.4, 0.6, 1.0, 1.0),
                error: Color::rgba(1.0, 0.45, 0.45, 1.0),
                success: Color::rgba(0.35, 0.85, 0.45, 1.0),
                focus_ring: Color::rgba(0.4, 0.6, 1.0, 1.0),
            },
        }
    }
}


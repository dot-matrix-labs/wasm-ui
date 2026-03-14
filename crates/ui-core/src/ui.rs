use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::accessibility::{A11yNode, A11yRole, A11yState, A11yTree};
use crate::batch::{Batch, Material, Quad, TextRun};
use crate::hit_test::{HitTestEntry, HitTestGrid};
use crate::input::{InputEvent, KeyCode, PointerButton};
use crate::text::TextBuffer;
use crate::text_measure::TextMeasure;
use crate::theme::Theme;
use crate::types::{Color, Rect, Vec2};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind {
    Label,
    Button,
    Checkbox,
    Radio,
    TextInput,
    Select,
    Group,
}

#[derive(Clone, Debug)]
pub struct WidgetInfo {
    pub id: u64,
    pub kind: WidgetKind,
    pub label: String,
    pub value: Option<String>,
    pub rect: Rect,
    pub state: A11yState,
}

#[derive(Clone, Debug)]
pub struct Layout {
    cursor: Vec2,
    width: f32,
    spacing: f32,
}

impl Layout {
    pub fn new(x: f32, y: f32, width: f32) -> Self {
        Self {
            cursor: Vec2::new(x, y),
            width,
            spacing: 10.0,
        }
    }

    pub fn next_rect(&mut self, height: f32) -> Rect {
        let rect = Rect::new(self.cursor.x, self.cursor.y, self.width, height);
        self.cursor.y += height + self.spacing;
        rect
    }
}

pub struct Ui {
    pub theme: Theme,
    pub batch: Batch,
    pub layout: Layout,
    pub widgets: Vec<WidgetInfo>,
    pub events: Vec<InputEvent>,
    pub focused: Option<u64>,
    pub hovered: Option<u64>,
    pub active: Option<u64>,
    pub dragging: Option<u64>,
    pub selection_anchor: Option<usize>,
    pub hit_test: HitTestGrid,
    pub scale: f32,
    pub clipboard_request: Option<String>,
    pub time_ms: f64,
    /// Number of rapid successive left-clicks on the same widget.
    pub click_count: u8,
    /// Timestamp of the last pointer-down, used to detect double/triple clicks.
    pub last_click_time: f64,
    /// Widget id that received the last click.
    pub last_click_id: Option<u64>,
    /// Horizontal scroll offsets per widget id (pixels scrolled to the right).
    pub scroll_offsets: std::collections::HashMap<u64, f32>,
    /// Whether the focused text input is in overwrite (insert-key toggle) mode.
    pub overwrite_mode: bool,
    /// Glyph advance-width provider.  Injected by the renderer layer so that
    /// `ui-core` stays renderer-agnostic.  Defaults to a monospace fallback.
    pub measure: TextMeasure,
}

impl Ui {
    pub fn new(width: f32, height: f32, theme: Theme) -> Self {
        let font_size = 15.0; // default; overridden via set_measure()
        Self {
            theme,
            batch: Batch::default(),
            layout: Layout::new(24.0, 24.0, width - 48.0),
            widgets: Vec::new(),
            events: Vec::new(),
            focused: None,
            hovered: None,
            active: None,
            dragging: None,
            selection_anchor: None,
            hit_test: HitTestGrid::new(width, height, 48.0),
            scale: 1.0,
            clipboard_request: None,
            time_ms: 0.0,
            click_count: 0,
            last_click_time: 0.0,
            last_click_id: None,
            scroll_offsets: std::collections::HashMap::new(),
            overwrite_mode: false,
            measure: TextMeasure::monospace(font_size * 0.6),
        }
    }

    /// Replace the glyph-metrics provider.  Call this once after loading the
    /// font (before the first frame that needs accurate hit-testing).
    pub fn set_measure(&mut self, measure: TextMeasure) {
        self.measure = measure;
    }

    pub fn begin_frame(
        &mut self,
        events: Vec<InputEvent>,
        width: f32,
        height: f32,
        scale: f32,
        time_ms: f64,
    ) {
        self.events = events;
        self.widgets.clear();
        self.batch.clear();
        self.layout = Layout::new(24.0, 24.0, width - 48.0);
        self.hit_test = HitTestGrid::new(width, height, 48.0);
        self.scale = scale;
        self.hovered = None;
        self.clipboard_request = None;
        self.time_ms = time_ms;
        // NOTE: selection_anchor is intentionally NOT cleared here.
        // It must persist across frames while the user is mid-drag.
        // It is cleared in apply_pointer_selection on PointerUp.
    }

    pub fn end_frame(&mut self) -> A11yTree {
        self.handle_keyboard_navigation();
        for widget in &self.widgets {
            self.hit_test.insert(HitTestEntry {
                id: widget.id,
                rect: widget.rect,
            });
        }
        A11yTree {
            root: A11yNode {
                id: 1,
                role: A11yRole::Form,
                name: "Form".to_string(),
                value: None,
                bounds: Rect::new(0.0, 0.0, self.layout.width, self.layout.cursor.y),
                state: A11yState::default(),
                children: self
                    .widgets
                    .iter()
                    .map(|w| A11yNode {
                        id: w.id,
                        role: widget_role(w.kind),
                        name: w.label.clone(),
                        value: w.value.clone(),
                        bounds: w.rect,
                        state: w.state.clone(),
                        children: Vec::new(),
                    })
                    .collect(),
            },
        }
    }

    fn handle_keyboard_navigation(&mut self) {
        let mut tab_pressed: Option<bool> = None;
        for event in &self.events {
            if let InputEvent::KeyDown { code: KeyCode::Tab, modifiers } = event {
                tab_pressed = Some(modifiers.shift);
            }
        }
        let shift = match tab_pressed {
            Some(value) => value,
            None => return,
        };
        if self.widgets.is_empty() {
            return;
        }
        let mut idx = self
            .widgets
            .iter()
            .position(|w| Some(w.id) == self.focused)
            .unwrap_or(0);
        if shift {
            if idx == 0 {
                idx = self.widgets.len() - 1;
            } else {
                idx -= 1;
            }
        } else {
            idx = (idx + 1) % self.widgets.len();
        }
        self.focused = Some(self.widgets[idx].id);
    }

    pub fn label(&mut self, text: &str) {
        let rect = self.layout.next_rect(24.0 * self.scale);
        self.widgets.push(WidgetInfo {
            id: self.hash_id(text),
            kind: WidgetKind::Label,
            label: text.to_string(),
            value: None,
            rect,
            state: A11yState::default(),
        });
        self.batch.text_runs.push(TextRun {
            rect,
            text: text.to_string(),
            color: self.theme.colors.text,
            font_size: 16.0 * self.theme.font_scale * self.scale,
            clip: None,
        });
    }

    pub fn label_colored(&mut self, text: &str, color: Color) {
        let rect = self.layout.next_rect(20.0 * self.scale);
        self.batch.text_runs.push(TextRun {
            rect,
            text: text.to_string(),
            color,
            font_size: 14.0 * self.theme.font_scale * self.scale,
            clip: None,
        });
    }

    pub fn button(&mut self, label: &str) -> bool {
        let rect = self.layout.next_rect(40.0 * self.scale);
        let id = self.hash_id(label);
        let hovered = self.rect_hovered(id, rect);
        let pressed = self.rect_pressed(id, rect);
        let clicked = pressed && self.rect_released(id, rect);

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::Button,
            label: label.to_string(),
            value: None,
            rect,
            state: A11yState {
                focused: self.focused == Some(id),
                disabled: false,
                invalid: false,
                required: false,
                expanded: false,
                selected: false,
            },
        });

        let bg = if pressed {
            self.theme.colors.primary
        } else if hovered {
            Color::rgba(
                self.theme.colors.primary.r,
                self.theme.colors.primary.g,
                self.theme.colors.primary.b,
                0.9,
            )
        } else {
            self.theme.colors.primary
        };

        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: bg,
                flags: 0,
            },
            Material::Solid,
            None,
        );
        self.batch.text_runs.push(TextRun {
            rect,
            text: label.to_string(),
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            font_size: 16.0 * self.theme.font_scale * self.scale,
            clip: None,
        });

        if clicked {
            self.focused = Some(id);
        }
        clicked
    }

    pub fn checkbox(&mut self, label: &str, value: &mut bool) -> bool {
        let rect = self.layout.next_rect(32.0 * self.scale);
        let id = self.hash_id(label);
        let clicked = self.rect_pressed(id, rect) && self.rect_released(id, rect);
        if clicked {
            *value = !*value;
            self.focused = Some(id);
        }
        let box_rect = Rect::new(rect.x, rect.y, rect.h, rect.h);
        self.batch.push_quad(
            Quad {
                rect: box_rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: self.theme.colors.surface,
                flags: 0,
            },
            Material::Solid,
            None,
        );
        if *value {
            self.batch.push_quad(
                Quad {
                    rect: Rect::new(rect.x + 6.0, rect.y + 6.0, rect.h - 12.0, rect.h - 12.0),
                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                    color: self.theme.colors.primary,
                    flags: 0,
                },
                Material::Solid,
                None,
            );
        }
        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + rect.h + 8.0, rect.y, rect.w - rect.h, rect.h),
            text: label.to_string(),
            color: self.theme.colors.text,
            font_size: 15.0 * self.theme.font_scale * self.scale,
            clip: None,
        });

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::Checkbox,
            label: label.to_string(),
            value: Some(value.to_string()),
            rect,
            state: A11yState {
                focused: self.focused == Some(id),
                disabled: false,
                invalid: false,
                required: false,
                expanded: false,
                selected: *value,
            },
        });

        clicked
    }

    pub fn select(&mut self, label: &str, options: &[String], value: &mut String) -> bool {
        let rect = self.layout.next_rect(36.0 * self.scale);
        let id = self.hash_id(label);
        let clicked = self.rect_pressed(id, rect) && self.rect_released(id, rect);
        if clicked {
            if let Some(pos) = options.iter().position(|v| v == value) {
                let next = (pos + 1) % options.len();
                *value = options[next].clone();
            }
            self.focused = Some(id);
        }
        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: self.theme.colors.surface,
                flags: 0,
            },
            Material::Solid,
            None,
        );
        let text = format!("{}: {}", label, value);
        self.batch.text_runs.push(TextRun {
            rect,
            text,
            color: self.theme.colors.text,
            font_size: 15.0 * self.theme.font_scale * self.scale,
            clip: None,
        });

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::Select,
            label: label.to_string(),
            value: Some(value.clone()),
            rect,
            state: A11yState {
                focused: self.focused == Some(id),
                disabled: false,
                invalid: false,
                required: false,
                expanded: false,
                selected: true,
            },
        });

        clicked
    }

    pub fn radio_group(&mut self, label: &str, options: &[String], selected: &mut usize) -> bool {
        self.ui_label_inline(label);
        let mut changed = false;
        for (idx, option) in options.iter().enumerate() {
            let rect = self.layout.next_rect(28.0 * self.scale);
            let id = self.hash_id(&format!("{}-{}", label, idx));
            let clicked = self.rect_pressed(id, rect) && self.rect_released(id, rect);
            if clicked {
                *selected = idx;
                self.focused = Some(id);
                changed = true;
            }
            let radius = rect.h * 0.35;
            let center = rect.center();
            let outer = Rect::new(center.x - radius, center.y - radius, radius * 2.0, radius * 2.0);
            self.batch.push_quad(
                Quad {
                    rect: outer,
                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                    color: self.theme.colors.surface,
                    flags: 0,
                },
                Material::Solid,
                None,
            );
            if *selected == idx {
                self.batch.push_quad(
                    Quad {
                        rect: Rect::new(center.x - radius * 0.5, center.y - radius * 0.5, radius, radius),
                        uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                        color: self.theme.colors.primary,
                        flags: 0,
                    },
                    Material::Solid,
                    None,
                );
            }
            self.batch.text_runs.push(TextRun {
                rect: Rect::new(rect.x + rect.h + 8.0, rect.y, rect.w - rect.h, rect.h),
                text: option.to_string(),
                color: self.theme.colors.text,
                font_size: 14.0 * self.theme.font_scale * self.scale,
                clip: None,
            });
            self.widgets.push(WidgetInfo {
                id,
                kind: WidgetKind::Radio,
                label: option.to_string(),
                value: Some(option.to_string()),
                rect,
                state: A11yState {
                    focused: self.focused == Some(id),
                    disabled: false,
                    invalid: false,
                    required: false,
                    expanded: false,
                    selected: *selected == idx,
                },
            });
        }
        changed
    }

    pub fn text_input(&mut self, label: &str, buffer: &mut TextBuffer, placeholder: &str) -> bool {
        self.text_input_impl(label, buffer, placeholder, false, false, None, 40.0 * self.scale)
    }

    pub fn text_input_multiline(
        &mut self,
        label: &str,
        buffer: &mut TextBuffer,
        placeholder: &str,
        height: f32,
    ) -> bool {
        self.text_input_impl(label, buffer, placeholder, true, false, None, height)
    }

    /// Password field — renders the value as bullet characters (`•`).
    pub fn text_input_password(
        &mut self,
        label: &str,
        buffer: &mut TextBuffer,
        placeholder: &str,
    ) -> bool {
        self.text_input_impl(label, buffer, placeholder, false, true, None, 40.0 * self.scale)
    }

    /// Text input with an optional inline error message rendered below the field.
    /// Pass `error: Some("message")` to show a red underline + error text.
    pub fn text_input_with_error(
        &mut self,
        label: &str,
        buffer: &mut TextBuffer,
        placeholder: &str,
        error: Option<&str>,
    ) -> bool {
        self.text_input_impl(label, buffer, placeholder, false, false, error, 40.0 * self.scale)
    }

    #[allow(clippy::too_many_arguments)]
    fn text_input_impl(
        &mut self,
        label: &str,
        buffer: &mut TextBuffer,
        placeholder: &str,
        multiline: bool,
        masked: bool,
        error: Option<&str>,
        height: f32,
    ) -> bool {
        let padding = 8.0;
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;

        let rect = self.layout.next_rect(height);
        let id = self.hash_id(label);
        let clicked = self.rect_pressed(id, rect) && self.rect_released(id, rect);
        if clicked {
            self.focused = Some(id);
        }
        let focused = self.focused == Some(id);
        if focused {
            // Check for Escape to blur before processing other events.
            let escape_pressed = self.events.iter().any(|e| {
                matches!(e, InputEvent::KeyDown { code: KeyCode::Escape, .. })
            });
            if escape_pressed {
                buffer.set_caret(buffer.caret().index); // collapse selection
                self.focused = None;
            } else {
                self.apply_text_events(buffer, multiline);
                self.apply_pointer_selection(id, rect, buffer);
                self.scroll_caret_into_view(id, rect, buffer, padding, font_size);
            }
        }

        let scroll_x = *self.scroll_offsets.get(&id).unwrap_or(&0.0);

        // Background quad
        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: self.theme.colors.surface,
                flags: 0,
            },
            Material::Solid,
            None,
        );

        // Focus ring (4 thin border quads)
        if focused {
            self.draw_focus_ring(rect);
        }

        // Error underline (thin red bottom border)
        let has_error = error.is_some();
        if has_error {
            let border_h = 2.0;
            let err_line = Rect::new(rect.x, rect.y + rect.h - border_h, rect.w, border_h);
            self.batch.push_quad(
                Quad {
                    rect: err_line,
                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                    color: self.theme.colors.error,
                    flags: 0,
                },
                Material::Solid,
                None,
            );
        }

        // Display string (masked or real)
        let is_empty = buffer.text().is_empty();
        let display_text = if is_empty {
            placeholder.to_string()
        } else if masked {
            // Render bullet per grapheme, matching the real glyph count exactly.
            use unicode_segmentation::UnicodeSegmentation;
            let n = buffer.text().graphemes(true).count();
            "•".repeat(n)
        } else {
            buffer.text().to_string()
        };
        let text_color = if is_empty {
            self.theme.colors.text_muted
        } else {
            self.theme.colors.text
        };

        if focused && !masked {
            self.draw_selection(id, rect, buffer, multiline, scroll_x);
        }

        // The TextRun is shifted left by scroll_x so long text scrolls.
        let text_rect = Rect::new(
            rect.x + padding - scroll_x,
            rect.y,
            rect.w - padding * 2.0 + scroll_x,
            rect.h,
        );
        self.batch.text_runs.push(TextRun {
            rect: text_rect,
            text: display_text,
            color: text_color,
            font_size,
            clip: Some(rect),
        });

        // Blinking caret
        if focused {
            let show_caret = (self.time_ms as u64 / 500) % 2 == 0;
            if show_caret {
                // For masked inputs use bullet width as advance; for real text use measure.
                let caret_pos = self.index_to_position(id, rect, buffer, buffer.caret().index, multiline, scroll_x, masked);
                let caret_rect = Rect::new(caret_pos.x, caret_pos.y, 1.5, line_height);
                self.batch.push_quad(
                    Quad {
                        rect: caret_rect,
                        uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                        color: self.theme.colors.text,
                        flags: 0,
                    },
                    Material::Solid,
                    Some(rect),
                );
            }
        }

        // Optional inline error message
        if let Some(err_msg) = error {
            let err_rect = self.layout.next_rect(18.0 * self.scale);
            self.batch.text_runs.push(TextRun {
                rect: err_rect,
                text: err_msg.to_string(),
                color: self.theme.colors.error,
                font_size: 12.0 * self.theme.font_scale * self.scale,
                clip: None,
            });
        }

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::TextInput,
            label: label.to_string(),
            value: Some(buffer.text().to_string()),
            rect,
            state: A11yState {
                focused: self.focused == Some(id),
                disabled: false,
                invalid: has_error,
                required: false,
                expanded: false,
                selected: false,
            },
        });

        clicked
    }

    pub fn tooltip(&mut self, target_label: &str, text: &str) {
        let id = self.hash_id(target_label);
        if self.hovered != Some(id) {
            return;
        }
        let rect = Rect::new(self.layout.width - 240.0, 16.0, 220.0, 60.0);
        self.batch.push_quad(
            Quad {
                rect,
                uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                color: Color::rgba(0.1, 0.1, 0.12, 0.9),
                flags: 0,
            },
            Material::Solid,
            None,
        );
        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + 8.0, rect.y + 8.0, rect.w - 16.0, rect.h - 16.0),
            text: text.to_string(),
            color: Color::rgba(1.0, 1.0, 1.0, 1.0),
            font_size: 13.0 * self.theme.font_scale * self.scale,
            clip: None,
        });
    }

    fn apply_text_events(&mut self, buffer: &mut TextBuffer, allow_newline: bool) {
        for event in &self.events {
            match event {
                InputEvent::TextInput(input) => {
                    if allow_newline || !input.text.contains('\n') {
                        if self.overwrite_mode && buffer.selection().is_none() {
                            // Overwrite mode: delete the character under the caret first.
                            buffer.delete_forward();
                        }
                        buffer.insert_text(&input.text);
                    }
                }
                InputEvent::KeyDown { code, modifiers } => match code {
                    // -------------------------------------------------------
                    // Deletion
                    // -------------------------------------------------------
                    KeyCode::Backspace if modifiers.ctrl || modifiers.alt => {
                        buffer.delete_word_backward();
                    }
                    KeyCode::Backspace => {
                        buffer.delete_backward();
                    }
                    KeyCode::Delete if modifiers.ctrl || modifiers.alt => {
                        buffer.delete_word_forward();
                    }
                    KeyCode::Delete => {
                        buffer.delete_forward();
                    }
                    // -------------------------------------------------------
                    // Horizontal movement
                    // -------------------------------------------------------
                    KeyCode::ArrowLeft if modifiers.ctrl || modifiers.alt => {
                        buffer.move_word_left(modifiers.shift);
                    }
                    KeyCode::ArrowLeft => {
                        // If there is a selection and shift is NOT held, collapse
                        // to the left edge (standard platform behaviour).
                        if buffer.selection().map(|s| !s.is_empty()).unwrap_or(false)
                            && !modifiers.shift
                        {
                            let sel = buffer.selection().unwrap().normalized();
                            buffer.set_caret(sel.start);
                        } else {
                            buffer.move_left(modifiers.shift);
                        }
                    }
                    KeyCode::ArrowRight if modifiers.ctrl || modifiers.alt => {
                        buffer.move_word_right(modifiers.shift);
                    }
                    KeyCode::ArrowRight => {
                        if buffer.selection().map(|s| !s.is_empty()).unwrap_or(false)
                            && !modifiers.shift
                        {
                            let sel = buffer.selection().unwrap().normalized();
                            buffer.set_caret(sel.end);
                        } else {
                            buffer.move_right(modifiers.shift);
                        }
                    }
                    // -------------------------------------------------------
                    // Vertical movement (multiline)
                    // -------------------------------------------------------
                    KeyCode::ArrowUp => {
                        // TODO: implement true line-up movement using
                        // index_to_position / position_to_index.
                        // For now fall through to Home as a safe stub.
                        buffer.move_to(0, modifiers.shift);
                    }
                    KeyCode::ArrowDown => {
                        // TODO: implement true line-down movement.
                        let len = buffer.grapheme_len();
                        buffer.move_to(len, modifiers.shift);
                    }
                    // -------------------------------------------------------
                    // Line start / end
                    // -------------------------------------------------------
                    KeyCode::Home if modifiers.ctrl => buffer.move_to(0, modifiers.shift),
                    KeyCode::Home => {
                        // Move to start of the current logical line.
                        buffer.move_to_line_start(modifiers.shift);
                    }
                    KeyCode::End if modifiers.ctrl => {
                        let len = buffer.grapheme_len();
                        buffer.move_to(len, modifiers.shift);
                    }
                    KeyCode::End => {
                        // Move to end of the current logical line.
                        buffer.move_to_line_end(modifiers.shift);
                    }
                    // -------------------------------------------------------
                    // Newline
                    // -------------------------------------------------------
                    KeyCode::Enter => {
                        if allow_newline {
                            buffer.insert_text("\n");
                        }
                    }
                    // -------------------------------------------------------
                    // Overwrite toggle
                    // -------------------------------------------------------
                    KeyCode::Insert => {
                        self.overwrite_mode = !self.overwrite_mode;
                    }
                    // -------------------------------------------------------
                    // Clipboard shortcuts
                    // -------------------------------------------------------
                    KeyCode::A if modifiers.ctrl || modifiers.meta => buffer.select_all(),
                    KeyCode::C if modifiers.ctrl || modifiers.meta => {
                        if let Some(text) = buffer.selected_text() {
                            self.clipboard_request = Some(text);
                        }
                    }
                    KeyCode::X if modifiers.ctrl || modifiers.meta => {
                        // cut_selection() atomically returns the text AND
                        // removes it in a single undo entry.
                        if let Some(text) = buffer.cut_selection() {
                            self.clipboard_request = Some(text);
                        }
                    }
                    // -------------------------------------------------------
                    // Undo / redo
                    // -------------------------------------------------------
                    KeyCode::Z if modifiers.ctrl || modifiers.meta => {
                        if modifiers.shift {
                            buffer.redo();
                        } else {
                            buffer.undo();
                        }
                    }
                    KeyCode::Y if modifiers.ctrl || modifiers.meta => {
                        buffer.redo();
                    }
                    _ => {}
                },
                // -----------------------------------------------------------
                // IME composition
                // -----------------------------------------------------------
                InputEvent::CompositionStart => {
                    buffer.begin_composition();
                }
                InputEvent::CompositionUpdate(text) => {
                    buffer.update_composition(text);
                }
                InputEvent::CompositionEnd(text) => {
                    buffer.end_composition(text);
                }
                // -----------------------------------------------------------
                // Paste from host clipboard (JS side calls handle_paste)
                // -----------------------------------------------------------
                InputEvent::Paste(text) => {
                    buffer.insert_text(text);
                }
                _ => {}
            }
        }
    }

    fn apply_pointer_selection(&mut self, id: u64, rect: Rect, buffer: &mut TextBuffer) {
        /// Maximum ms gap between clicks to count as a multi-click sequence.
        const DOUBLE_CLICK_MS: f64 = 400.0;

        // Collect the events we need (borrow-checker: copy out what we need).
        let events: Vec<InputEvent> = self.events.clone();

        for event in &events {
            match event {
                InputEvent::PointerDown(ev) => {
                    if rect.contains(ev.pos) && ev.button == Some(PointerButton::Left) {
                        let scroll_x = *self.scroll_offsets.get(&id).unwrap_or(&0.0);
                        // Adjust the pointer X for the scroll offset before hit-testing.
                        let adjusted_pos = Vec2::new(ev.pos.x + scroll_x, ev.pos.y);
                        let idx = self.position_to_index(id, rect, buffer, adjusted_pos);

                        // --- Multi-click detection ---
                        let same_target = self.last_click_id == Some(id);
                        let within_time = (self.time_ms - self.last_click_time) < DOUBLE_CLICK_MS;
                        if same_target && within_time {
                            self.click_count = self.click_count.saturating_add(1);
                        } else {
                            self.click_count = 1;
                        }
                        self.last_click_time = self.time_ms;
                        self.last_click_id = Some(id);

                        match self.click_count {
                            1 => {
                                // Single click: place caret, begin drag.
                                buffer.set_caret(idx);
                                self.dragging = Some(id);
                                self.selection_anchor = Some(idx);
                            }
                            2 => {
                                // Double click: select the word at the click position.
                                buffer.select_word_at(idx);
                                self.dragging = None; // no drag after word-select
                                self.selection_anchor = None;
                            }
                            _ => {
                                // Triple (or more) click: select the whole logical line.
                                buffer.select_line_at(idx);
                                self.dragging = None;
                                self.selection_anchor = None;
                            }
                        }
                    }
                }
                InputEvent::PointerMove(ev) => {
                    if self.dragging == Some(id) {
                        let scroll_x = *self.scroll_offsets.get(&id).unwrap_or(&0.0);
                        let adjusted_pos = Vec2::new(ev.pos.x + scroll_x, ev.pos.y);
                        let idx = self.position_to_index(id, rect, buffer, adjusted_pos);
                        let start = self.selection_anchor.unwrap_or(buffer.caret().index);
                        buffer.set_selection(start, idx);
                        // Drag autoscroll: nudge scroll_x if pointer is outside rect.
                        let overshoot_right = ev.pos.x - (rect.x + rect.w);
                        let overshoot_left  = rect.x - ev.pos.x;
                        let nudge = 8.0_f32; // px per frame
                        if overshoot_right > 0.0 {
                            let new_sx = scroll_x + nudge;
                            self.scroll_offsets.insert(id, new_sx);
                        } else if overshoot_left > 0.0 {
                            let new_sx = (scroll_x - nudge).max(0.0);
                            self.scroll_offsets.insert(id, new_sx);
                        }
                    }
                }
                InputEvent::PointerUp(ev) => {
                    if self.dragging == Some(id) && ev.button == Some(PointerButton::Left) {
                        self.dragging = None;
                        self.selection_anchor = None;
                    }
                }
                _ => {}
            }
        }
    }

    /// Adjust `scroll_offsets[id]` so the caret remains inside the visible
    /// horizontal window of `rect`.
    fn scroll_caret_into_view(
        &mut self,
        id: u64,
        rect: Rect,
        buffer: &TextBuffer,
        padding: f32,
        font_size: f32,
    ) {
        let scroll_x = *self.scroll_offsets.get(&id).unwrap_or(&0.0);
        // Use real advance widths up to the caret position on its line.
        let caret_idx = buffer.caret().index;
        // Find the line the caret is on and how far along it we are.
        let mut remaining = caret_idx;
        let mut caret_line_text = "";
        let mut caret_col = 0usize;
        for line in buffer.text().split('\n') {
            let graphemes = line.graphemes(true).count();
            if remaining <= graphemes {
                caret_line_text = line;
                caret_col = remaining;
                break;
            }
            remaining = remaining.saturating_sub(graphemes + 1);
        }
        // Sum advances up to caret_col on that line.
        let caret_x_in_text: f32 = caret_line_text
            .graphemes(true)
            .take(caret_col)
            .flat_map(|g| g.chars())
            .map(|ch| self.measure.advance(ch))
            .sum();
        let _ = font_size; // reserved for future line-height calcs

        let visible_left  = scroll_x;
        let visible_right = scroll_x + rect.w - padding * 2.0;
        let new_scroll = if caret_x_in_text < visible_left {
            (caret_x_in_text - padding).max(0.0)
        } else if caret_x_in_text > visible_right {
            caret_x_in_text - (rect.w - padding * 3.0)
        } else {
            scroll_x
        };
        self.scroll_offsets.insert(id, new_scroll.max(0.0));
    }

    /// Draw four thin quads forming a focus ring just inside `rect`.
    fn draw_focus_ring(&mut self, rect: Rect) {
        let t = 2.0_f32; // border thickness in logical pixels
        let color = self.theme.colors.focus_ring;
        let borders = [
            Rect::new(rect.x,             rect.y,             rect.w, t),     // top
            Rect::new(rect.x,             rect.y + rect.h - t, rect.w, t),   // bottom
            Rect::new(rect.x,             rect.y,             t, rect.h),     // left
            Rect::new(rect.x + rect.w - t, rect.y,           t, rect.h),     // right
        ];
        for border in borders {
            self.batch.push_quad(
                Quad { rect: border, uv: Rect::new(0.0, 0.0, 1.0, 1.0), color, flags: 0 },
                Material::Solid,
                None,
            );
        }
    }

    fn position_to_index(&self, _id: u64, rect: Rect, buffer: &TextBuffer, pos: Vec2) -> usize {
        let padding = 8.0;
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let scroll_x = 0.0_f32; // TODO: pass id scroll offset when needed for click accuracy
        let x = (pos.x - rect.x - padding + scroll_x).max(0.0);
        let y = (pos.y - rect.y - padding).max(0.0);
        let target_line = (y / line_height).floor() as usize;
        let mut index = 0usize;
        for (line_idx, line_text) in buffer.text().split('\n').enumerate() {
            if line_idx == target_line {
                // Use real glyph advances for this line.
                index += self.measure.x_to_grapheme_index(line_text, x);
                return index;
            }
            index += line_text.graphemes(true).count() + 1; // +1 for \n
        }
        buffer.grapheme_len()
    }

    fn index_to_position(
        &self,
        _id: u64,
        rect: Rect,
        buffer: &TextBuffer,
        index: usize,
        _multiline: bool,
        scroll_x: f32,
        masked: bool,
    ) -> Vec2 {
        let padding = 8.0;
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let mut remaining = index;
        let mut line = 0;
        for line_text in buffer.text().split('\n') {
            let graphemes: Vec<&str> = line_text.graphemes(true).collect();
            let count = graphemes.len();
            if remaining <= count {
                // Sum advance widths up to `remaining` graphemes on this line.
                let x_in_text: f32 = if masked {
                    // For password fields, use the bullet glyph width.
                    self.measure.advance('•') * remaining as f32
                } else {
                    graphemes[..remaining]
                        .iter()
                        .flat_map(|g| g.chars())
                        .map(|ch| self.measure.advance(ch))
                        .sum()
                };
                let x = rect.x + padding + x_in_text - scroll_x;
                let y = rect.y + padding + line as f32 * line_height;
                return Vec2::new(x, y);
            }
            remaining = remaining.saturating_sub(count + 1);
            line += 1;
        }
        Vec2::new(rect.x + padding, rect.y + padding)
    }

    fn draw_selection(
        &mut self,
        _id: u64,
        rect: Rect,
        buffer: &TextBuffer,
        _multiline: bool,
        scroll_x: f32,
    ) {
        let selection = match buffer.selection() {
            Some(sel) if !sel.is_empty() => sel.normalized(),
            _ => return,
        };
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let padding = 8.0;
        let lines: Vec<&str> = buffer.text().split('\n').collect();
        let (start_line, start_col) = self.index_to_line_col(&lines, selection.start);
        let (end_line, end_col)     = self.index_to_line_col(&lines, selection.end);

        for line_idx in start_line..=end_line {
            let line_text = lines.get(line_idx).copied().unwrap_or("");
            let graphemes: Vec<&str> = line_text.graphemes(true).collect();
            let line_len = graphemes.len();
            let (col_start, col_end) = if line_idx == start_line && line_idx == end_line {
                (start_col, end_col)
            } else if line_idx == start_line {
                (start_col, line_len)
            } else if line_idx == end_line {
                (0, end_col)
            } else {
                (0, line_len)
            };
            if col_start == col_end {
                continue;
            }
            // Use real advances for selection rect width.
            let x_start: f32 = graphemes[..col_start]
                .iter().flat_map(|g| g.chars()).map(|c| self.measure.advance(c)).sum();
            let x_end:   f32 = graphemes[..col_end]
                .iter().flat_map(|g| g.chars()).map(|c| self.measure.advance(c)).sum();
            let x = rect.x + padding + x_start - scroll_x;
            let y = rect.y + padding + line_idx as f32 * line_height;
            let w = x_end - x_start;
            let sel_rect = Rect::new(x, y, w, line_height);
            self.batch.push_quad(
                Quad {
                    rect: sel_rect,
                    uv: Rect::new(0.0, 0.0, 1.0, 1.0),
                    color: Color::rgba(0.2, 0.45, 0.9, 0.25),
                    flags: 0,
                },
                Material::Solid,
                Some(rect),
            );
        }
    }

    fn index_to_line_col(&self, lines: &[&str], mut index: usize) -> (usize, usize) {
        for (line_idx, line) in lines.iter().enumerate() {
            let count = line.graphemes(true).count();
            if index <= count {
                return (line_idx, index);
            }
            index = index.saturating_sub(count + 1);
        }
        let last = lines.len().saturating_sub(1);
        (last, 0)
    }

    fn rect_hovered(&mut self, id: u64, rect: Rect) -> bool {
        let mut hovered = false;
        for event in &self.events {
            if let InputEvent::PointerMove(ev) = event {
                if rect.contains(ev.pos) {
                    hovered = true;
                }
            }
        }
        if hovered {
            self.hovered = Some(id);
        }
        hovered
    }

    fn rect_pressed(&mut self, id: u64, rect: Rect) -> bool {
        for event in &self.events {
            if let InputEvent::PointerDown(ev) = event {
                if rect.contains(ev.pos) && ev.button == Some(PointerButton::Left) {
                    self.active = Some(id);
                    return true;
                }
            }
        }
        false
    }

    fn rect_released(&mut self, id: u64, rect: Rect) -> bool {
        for event in &self.events {
            if let InputEvent::PointerUp(ev) = event {
                if rect.contains(ev.pos) && ev.button == Some(PointerButton::Left) {
                    if self.active == Some(id) {
                        self.active = None;
                    }
                    return true;
                }
            }
        }
        false
    }

    fn hash_id(&self, label: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        label.hash(&mut hasher);
        hasher.finish()
    }

}

fn widget_role(kind: WidgetKind) -> A11yRole {
    match kind {
        WidgetKind::Label => A11yRole::Label,
        WidgetKind::Button => A11yRole::Button,
        WidgetKind::Checkbox => A11yRole::CheckBox,
        WidgetKind::Radio => A11yRole::RadioButton,
        WidgetKind::TextInput => A11yRole::TextBox,
        WidgetKind::Select => A11yRole::ComboBox,
        WidgetKind::Group => A11yRole::Group,
    }
}

impl Ui {
    fn ui_label_inline(&mut self, text: &str) {
        let rect = self.layout.next_rect(22.0 * self.scale);
        self.batch.text_runs.push(TextRun {
            rect,
            text: text.to_string(),
            color: self.theme.colors.text,
            font_size: 13.0 * self.theme.font_scale * self.scale,
            clip: None,
        });
    }
}

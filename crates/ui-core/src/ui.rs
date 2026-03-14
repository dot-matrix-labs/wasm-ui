use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::accessibility::{A11yNode, A11yRole, A11yState, A11yTree};
use crate::batch::{Batch, Material, Quad, TextRun};
use crate::hit_test::{HitTestEntry, HitTestGrid};
use crate::input::{InputEvent, KeyCode, PointerButton};
use crate::metrics::{GlyphMetrics, MonospaceMetrics};
use crate::text::TextBuffer;
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
    /// When Some, we are inside a begin_row/end_row block.
    /// Stores (row_start_x, row_start_y, max_height_seen_so_far, gap).
    row_state: Option<(f32, f32, f32, f32)>,
}

impl Layout {
    pub fn new(x: f32, y: f32, width: f32) -> Self {
        Self {
            cursor: Vec2::new(x, y),
            width,
            spacing: 10.0,
            row_state: None,
        }
    }

    pub fn next_rect(&mut self, height: f32) -> Rect {
        if let Some((_, _, ref mut max_h, gap)) = self.row_state {
            // Horizontal layout: place item at current cursor, advance X.
            let rect = Rect::new(self.cursor.x, self.cursor.y, self.width, height);
            // Width will be overridden by caller in row context; here just provide
            // a single-item wide rect and let begin/end_row manage widths.
            if height > *max_h {
                *max_h = height;
            }
            self.cursor.x += self.width + gap;
            rect
        } else {
            let rect = Rect::new(self.cursor.x, self.cursor.y, self.width, height);
            self.cursor.y += height + self.spacing;
            rect
        }
    }

    /// Reserve a rect of explicit width (used inside row containers).
    pub fn next_rect_sized(&mut self, width: f32, height: f32) -> Rect {
        if let Some((_, _, ref mut max_h, gap)) = self.row_state {
            let rect = Rect::new(self.cursor.x, self.cursor.y, width, height);
            if height > *max_h {
                *max_h = height;
            }
            self.cursor.x += width + gap;
            rect
        } else {
            let rect = Rect::new(self.cursor.x, self.cursor.y, width, height);
            self.cursor.y += height + self.spacing;
            rect
        }
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
    /// 1 = single, 2 = double (select word), 3+ = triple (select line).
    pub click_count: u8,
    /// Timestamp of the last pointer-down, used to detect double/triple clicks.
    pub last_click_time: f64,
    /// Widget id that received the last click, used to reset count on target change.
    pub last_click_id: Option<u64>,
    /// Scroll offsets per widget id (horizontal pixel offset into the text).
    pub scroll_offsets: std::collections::HashMap<u64, f32>,
    /// Whether the focused text input is in overwrite (insert-key toggle) mode.
    pub overwrite_mode: bool,
    /// Task 3.5: When true, skip animated focus indicators (prefers-reduced-motion).
    pub reduce_motion: bool,
    /// Task 3.6: Safe area insets (top, right, bottom, left) in logical pixels.
    pub safe_area_insets: (f32, f32, f32, f32),
    /// Task 3.3: The widget id of the currently open dropdown (if any).
    pub open_dropdown: Option<u64>,
    /// Task 3.3: Selected index inside the open dropdown.
    pub dropdown_selected: usize,
    /// Task 3.2: Scroll offsets per scroll-container id (vertical pixel offset).
    pub container_scroll: std::collections::HashMap<u64, f32>,
    /// Task 2.3: Proportional Text Metrics — pluggable glyph advance provider.
    pub glyph_metrics: Box<dyn GlyphMetrics + Send + Sync>,
}

impl Ui {
    pub fn new(width: f32, height: f32, theme: Theme) -> Self {
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
            reduce_motion: false,
            safe_area_insets: (0.0, 0.0, 0.0, 0.0),
            open_dropdown: None,
            dropdown_selected: 0,
            container_scroll: std::collections::HashMap::new(),
            glyph_metrics: Box::new(MonospaceMetrics),
        }
    }

    /// Task 3.5: Called by the host to indicate prefers-reduced-motion.
    pub fn set_reduce_motion(&mut self, reduce: bool) {
        self.reduce_motion = reduce;
    }

    /// Task 3.6: Set safe area insets received from the host (CSS env() values).
    pub fn set_safe_area_insets(&mut self, top: f32, right: f32, bottom: f32, left: f32) {
        self.safe_area_insets = (top, right, bottom, left);
        let (t, _r, _b, l) = self.safe_area_insets;
        // Recompute the layout origin to respect insets.
        self.layout = Layout::new(24.0 + l, 24.0 + t, self.layout.width);
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

    /// Render a non-interactive text label.
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

    /// Render a clickable button. Returns `true` on the frame it is clicked.
    pub fn button(&mut self, label: &str) -> bool {
        let rect = self.layout.next_rect(40.0 * self.scale);
        let id = self.hash_id(label);
        // Task 3.4: expand touch target on mobile
        let touch_rect = self.expand_touch_target(rect);
        let hovered = self.rect_hovered(id, touch_rect);
        let pressed = self.rect_pressed(id, touch_rect);
        let clicked = pressed && self.rect_released(id, touch_rect);

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
        // Task 3.5: focus ring
        self.draw_focus_ring_if_focused(id, rect);
        clicked
    }

    /// Render a checkbox toggle. Returns `true` when the value changes.
    pub fn checkbox(&mut self, label: &str, value: &mut bool) -> bool {
        let rect = self.layout.next_rect(32.0 * self.scale);
        let id = self.hash_id(label);
        // Task 3.4: expand touch target
        let touch_rect = self.expand_touch_target(rect);
        let clicked = self.rect_pressed(id, touch_rect) && self.rect_released(id, touch_rect);
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

        // Task 3.5: focus ring
        self.draw_focus_ring_if_focused(id, rect);
        clicked
    }

    pub fn select(&mut self, label: &str, options: &[String], value: &mut String) -> bool {
        let rect = self.layout.next_rect(36.0 * self.scale);
        let id = self.hash_id(label);
        // Task 3.4: expand touch target
        let touch_rect = self.expand_touch_target(rect);
        let clicked = self.rect_pressed(id, touch_rect) && self.rect_released(id, touch_rect);
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

        // Task 3.5: focus ring
        self.draw_focus_ring_if_focused(id, rect);
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

    /// Render a single-line text input. Returns `true` when the buffer changes.
    pub fn text_input(&mut self, label: &str, buffer: &mut TextBuffer, placeholder: &str) -> bool {
        self.text_input_impl(label, buffer, placeholder, false, false, 40.0 * self.scale)
    }

    /// Render a password input with masked characters. Returns `true` when the buffer changes.
    pub fn password_input(&mut self, label: &str, buffer: &mut TextBuffer, placeholder: &str) -> bool {
        self.text_input_impl(label, buffer, placeholder, false, true, 40.0 * self.scale)
    }

    pub fn text_input_multiline(
        &mut self,
        label: &str,
        buffer: &mut TextBuffer,
        placeholder: &str,
        height: f32,
    ) -> bool {
        self.text_input_impl(label, buffer, placeholder, true, false, height)
    }

    fn text_input_impl(
        &mut self,
        label: &str,
        buffer: &mut TextBuffer,
        placeholder: &str,
        multiline: bool,
        masked: bool,
        height: f32,
    ) -> bool {
        let rect = self.layout.next_rect(height);
        let id = self.hash_id(label);
        // Task 3.4: expand touch target for hit testing
        let touch_rect = self.expand_touch_target(rect);
        let clicked = self.rect_pressed(id, touch_rect) && self.rect_released(id, touch_rect);
        if clicked {
            self.focused = Some(id);
        }
        let focused = self.focused == Some(id);
        if focused {
            self.apply_text_events(buffer, multiline);
            self.apply_pointer_selection(id, rect, buffer);
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
        let content = if buffer.text().is_empty() {
            placeholder.to_string()
        } else if masked {
            "\u{2022}".repeat(buffer.grapheme_len())
        } else {
            buffer.text().to_string()
        };
        let color = if buffer.text().is_empty() {
            self.theme.colors.text_muted
        } else {
            self.theme.colors.text
        };
        if focused {
            self.draw_selection(rect, buffer, multiline);
        }

        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + 8.0, rect.y, rect.w - 16.0, rect.h),
            text: content,
            color,
            font_size: 15.0 * self.theme.font_scale * self.scale,
            clip: Some(rect),
        });

        if focused {
            let show_caret = (self.time_ms as u64 / 500).is_multiple_of(2);
            if show_caret {
                let caret_pos = self.index_to_position(rect, buffer, buffer.caret().index, multiline);
                let caret_rect = Rect::new(caret_pos.x, caret_pos.y, 1.5, 18.0 * self.scale);
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

        self.widgets.push(WidgetInfo {
            id,
            kind: WidgetKind::TextInput,
            label: label.to_string(),
            value: Some(buffer.text().to_string()),
            rect,
            state: A11yState {
                focused,
                disabled: false,
                invalid: false,
                required: false,
                expanded: false,
                selected: false,
            },
        });

        // Task 3.5: focus ring
        self.draw_focus_ring_if_focused(id, rect);
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
                        let idx = self.position_to_index(rect, buffer, ev.pos);

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
                        let idx = self.position_to_index(rect, buffer, ev.pos);
                        let start = self.selection_anchor.unwrap_or(buffer.caret().index);
                        buffer.set_selection(start, idx);
                        // TODO: if ev.pos is outside rect horizontally, nudge
                        // self.scroll_offsets[id] to auto-scroll the viewport.
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

    fn position_to_index(&self, rect: Rect, buffer: &TextBuffer, pos: Vec2) -> usize {
        // Task 2.3: Proportional Text Metrics — use actual glyph advances.
        let padding = 8.0;
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let x = (pos.x - rect.x - padding).max(0.0);
        let y = (pos.y - rect.y - padding).max(0.0);
        let line = (y / line_height).floor() as usize;
        let mut index = 0usize;
        for (line_idx, line_text) in buffer.text().split('\n').enumerate() {
            let graphemes_count = line_text.graphemes(true).count();
            if line_idx == line {
                // Build prefix sums for this line and binary-search for x.
                let prefix = self.glyph_metrics.advance_prefix_sums(line_text, font_size);
                let col = self.glyph_metrics.index_for_x(&prefix, x);
                index += col.min(graphemes_count);
                return index;
            }
            index += graphemes_count + 1;
        }
        buffer.grapheme_len()
    }

    fn index_to_position(&self, rect: Rect, buffer: &TextBuffer, index: usize, _multiline: bool) -> Vec2 {
        // Task 2.3: Proportional Text Metrics — use advance prefix sums.
        let padding = 8.0;
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let mut remaining = index;
        for (line, line_text) in buffer.text().split('\n').enumerate() {
            let graphemes = line_text.graphemes(true).count();
            if remaining <= graphemes {
                // prefix[remaining] gives the pixel x-offset for this grapheme.
                let prefix = self.glyph_metrics.advance_prefix_sums(line_text, font_size);
                let x_off = prefix.get(remaining).copied().unwrap_or(0.0);
                let x = rect.x + padding + x_off;
                let y = rect.y + padding + line as f32 * line_height;
                return Vec2::new(x, y);
            }
            remaining = remaining.saturating_sub(graphemes + 1);
        }
        Vec2::new(rect.x + padding, rect.y + padding)
    }

    fn draw_selection(&mut self, rect: Rect, buffer: &TextBuffer, _multiline: bool) {
        // Task 2.3: Proportional Text Metrics — use per-line advance prefix sums.
        let selection = match buffer.selection() {
            Some(sel) if !sel.is_empty() => sel.normalized(),
            _ => return,
        };
        let font_size = 15.0 * self.theme.font_scale * self.scale;
        let line_height = font_size * 1.4;
        let padding = 8.0;
        let lines: Vec<&str> = buffer.text().split('\n').collect();
        let (start_line, start_col) = self.index_to_line_col(&lines, selection.start);
        let (end_line, end_col) = self.index_to_line_col(&lines, selection.end);

        for line in start_line..=end_line {
            let line_text = lines.get(line).copied().unwrap_or("");
            let line_len = line_text.graphemes(true).count();
            let prefix = self.glyph_metrics.advance_prefix_sums(line_text, font_size);
            let (col_start, col_end) = if line == start_line && line == end_line {
                (start_col, end_col)
            } else if line == start_line {
                (start_col, line_len)
            } else if line == end_line {
                (0, end_col)
            } else {
                (0, line_len)
            };
            if col_start == col_end {
                continue;
            }
            let x_start = prefix.get(col_start).copied().unwrap_or(0.0);
            let x_end = prefix.get(col_end).copied().unwrap_or(0.0);
            let x = rect.x + padding + x_start;
            let y = rect.y + padding + line as f32 * line_height;
            let w = (x_end - x_start).max(1.0);
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

    /// Task 3.5: Draw a 2-pixel focus ring outline around `rect` when the
    /// widget with `id` is focused.
    fn draw_focus_ring_if_focused(&mut self, id: u64, rect: Rect) {
        if self.focused != Some(id) {
            return;
        }
        let color = self.theme.colors.focus_ring;
        let w = 2.0;
        // Top bar
        self.batch.push_quad(Quad { rect: Rect::new(rect.x - w, rect.y - w, rect.w + w * 2.0, w), uv: Rect::new(0.0,0.0,1.0,1.0), color, flags: 0 }, Material::Solid, None);
        // Bottom bar
        self.batch.push_quad(Quad { rect: Rect::new(rect.x - w, rect.y + rect.h, rect.w + w * 2.0, w), uv: Rect::new(0.0,0.0,1.0,1.0), color, flags: 0 }, Material::Solid, None);
        // Left bar
        self.batch.push_quad(Quad { rect: Rect::new(rect.x - w, rect.y - w, w, rect.h + w * 2.0), uv: Rect::new(0.0,0.0,1.0,1.0), color, flags: 0 }, Material::Solid, None);
        // Right bar
        self.batch.push_quad(Quad { rect: Rect::new(rect.x + rect.w, rect.y - w, w, rect.h + w * 2.0), uv: Rect::new(0.0,0.0,1.0,1.0), color, flags: 0 }, Material::Solid, None);
    }

    /// Task 3.4: Expand `rect` to a minimum 44pt touch target for hit-testing
    /// on high-DPI / mobile (scale > 1.5). Returns the expanded rect.
    fn expand_touch_target(&self, rect: Rect) -> Rect {
        if self.scale <= 1.5 {
            return rect;
        }
        let min_size = 44.0 * self.scale;
        let extra_w = (min_size - rect.w).max(0.0);
        let extra_h = (min_size - rect.h).max(0.0);
        Rect::new(
            rect.x - extra_w * 0.5,
            rect.y - extra_h * 0.5,
            rect.w + extra_w,
            rect.h + extra_h,
        )
    }

    // -------------------------------------------------------------------------
    // Task 3.1: Horizontal row containers
    // -------------------------------------------------------------------------

    /// Begin a horizontal row layout. Children will be placed side-by-side.
    /// `gap` is the pixel spacing between children.
    pub fn begin_row(&mut self, gap: f32) {
        self.layout.row_state = Some((
            self.layout.cursor.x,
            self.layout.cursor.y,
            0.0,
            gap,
        ));
    }

    /// End the current horizontal row layout and advance the vertical cursor
    /// past the tallest child.
    pub fn end_row(&mut self) {
        if let Some((start_x, start_y, max_h, _)) = self.layout.row_state.take() {
            self.layout.cursor.x = start_x;
            self.layout.cursor.y = start_y + max_h + self.layout.spacing;
        }
    }

    // -------------------------------------------------------------------------
    // Task 3.2: Scroll containers (stub — scissor rect support TODO in renderer)
    // -------------------------------------------------------------------------

    /// Begin a scroll container of the given visual `height`.
    /// Returns the visible rect. Scrolling is tracked per `id`.
    pub fn begin_scroll(&mut self, id: u64, height: f32) -> Rect {
        let rect = self.layout.next_rect(height);
        // Draw background for the scroll area
        self.batch.push_quad(
            Quad { rect, uv: Rect::new(0.0, 0.0, 1.0, 1.0), color: self.theme.colors.surface, flags: 0 },
            Material::Solid,
            None,
        );
        // Handle wheel events inside this rect
        for event in &self.events.clone() {
            if let crate::input::InputEvent::PointerWheel { pos, delta, .. } = event {
                if rect.contains(*pos) {
                    let offset = self.container_scroll.entry(id).or_insert(0.0);
                    *offset = (*offset + delta.y).max(0.0);
                }
            }
        }
        rect
    }

    /// End the scroll container. `clip_rect` is the rect returned by begin_scroll.
    pub fn end_scroll(&mut self, _clip_rect: Rect) {
        // TODO: pop scissor rect from renderer when scissor support is added.
    }

    // -------------------------------------------------------------------------
    // Task 3.3: Proper dropdown / select widget
    // -------------------------------------------------------------------------

    /// Draw a dropdown (select) widget. Returns true when the value changes.
    pub fn dropdown(&mut self, label: &str, options: &[String], value: &mut String) -> bool {
        let rect = self.layout.next_rect(36.0 * self.scale);
        let id = self.hash_id(label);
        let touch_rect = self.expand_touch_target(rect);
        let clicked = self.rect_pressed(id, touch_rect) && self.rect_released(id, touch_rect);
        let is_open = self.open_dropdown == Some(id);

        if clicked {
            if is_open {
                self.open_dropdown = None;
            } else {
                self.open_dropdown = Some(id);
                // Set selected index to current value
                self.dropdown_selected = options.iter().position(|v| v == value).unwrap_or(0);
            }
            self.focused = Some(id);
        }

        // Handle keyboard when open
        if is_open && self.focused == Some(id) {
            for event in &self.events.clone() {
                match event {
                    crate::input::InputEvent::KeyDown { code, .. } => match code {
                        KeyCode::ArrowDown => {
                            if self.dropdown_selected + 1 < options.len() {
                                self.dropdown_selected += 1;
                            }
                        }
                        KeyCode::ArrowUp => {
                            if self.dropdown_selected > 0 {
                                self.dropdown_selected -= 1;
                            }
                        }
                        KeyCode::Enter => {
                            *value = options[self.dropdown_selected].clone();
                            self.open_dropdown = None;
                        }
                        KeyCode::Escape => {
                            self.open_dropdown = None;
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        // Handle click outside to close
        if is_open {
            for event in &self.events.clone() {
                if let crate::input::InputEvent::PointerDown(ev) = event {
                    if ev.button == Some(crate::input::PointerButton::Left) && !rect.contains(ev.pos) {
                        // Check if the click is inside the floating list
                        let list_h = options.len() as f32 * 32.0 * self.scale;
                        let list_rect = Rect::new(rect.x, rect.y + rect.h, rect.w, list_h);
                        if !list_rect.contains(ev.pos) {
                            self.open_dropdown = None;
                        }
                    }
                }
            }
        }

        // Draw button face
        self.batch.push_quad(
            Quad { rect, uv: Rect::new(0.0,0.0,1.0,1.0), color: self.theme.colors.surface, flags: 0 },
            Material::Solid,
            None,
        );
        let text = format!("{}: {} ▾", label, value);
        self.batch.text_runs.push(TextRun {
            rect: Rect::new(rect.x + 8.0, rect.y, rect.w - 16.0, rect.h),
            text,
            color: self.theme.colors.text,
            font_size: 15.0 * self.theme.font_scale * self.scale,
            clip: None,
        });

        self.draw_focus_ring_if_focused(id, rect);

        let mut changed = false;

        // Draw floating option list when open
        if self.open_dropdown == Some(id) {
            let item_h = 32.0 * self.scale;
            let list_h = options.len() as f32 * item_h;
            let list_rect = Rect::new(rect.x, rect.y + rect.h, rect.w, list_h);

            // Background
            self.batch.push_quad(
                Quad { rect: list_rect, uv: Rect::new(0.0,0.0,1.0,1.0), color: self.theme.colors.surface, flags: 0 },
                Material::Solid,
                None,
            );

            for (i, option) in options.iter().enumerate() {
                let item_rect = Rect::new(list_rect.x, list_rect.y + i as f32 * item_h, list_rect.w, item_h);
                let item_id = self.hash_id(&format!("{}-opt-{}", label, i));
                let item_hovered = self.rect_hovered(item_id, item_rect);

                if item_hovered {
                    self.batch.push_quad(
                        Quad { rect: item_rect, uv: Rect::new(0.0,0.0,1.0,1.0), color: Color::rgba(self.theme.colors.primary.r, self.theme.colors.primary.g, self.theme.colors.primary.b, 0.15), flags: 0 },
                        Material::Solid,
                        None,
                    );
                }
                if i == self.dropdown_selected {
                    self.batch.push_quad(
                        Quad { rect: Rect::new(item_rect.x, item_rect.y, 3.0, item_rect.h), uv: Rect::new(0.0,0.0,1.0,1.0), color: self.theme.colors.primary, flags: 0 },
                        Material::Solid,
                        None,
                    );
                }

                // Check click on item
                let item_pressed = self.rect_pressed(item_id, item_rect);
                let item_released = self.rect_released(item_id, item_rect);
                if item_pressed && item_released {
                    *value = option.clone();
                    self.open_dropdown = None;
                    changed = true;
                }

                self.batch.text_runs.push(TextRun {
                    rect: Rect::new(item_rect.x + 12.0, item_rect.y, item_rect.w - 16.0, item_rect.h),
                    text: option.clone(),
                    color: self.theme.colors.text,
                    font_size: 15.0 * self.theme.font_scale * self.scale,
                    clip: Some(list_rect),
                });
            }
        }

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
                expanded: self.open_dropdown == Some(id),
                selected: true,
            },
        });

        changed
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

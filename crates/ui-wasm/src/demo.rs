use serde::Serialize;
use wasm_bindgen::JsValue;

use ui_core::app::FormApp;
use ui_core::batch::Batch;
use ui_core::form::{FieldSchema, FieldType, FieldValue, Form, FormEvent, FormPath, FormSchema};
use ui_core::input::{InputEvent, KeyCode, Modifiers, PointerButton, PointerEvent, TextInputEvent};
use ui_core::text::TextBuffer;
use ui_core::theme::Theme;
use ui_core::ui::Ui;
use ui_core::validation::ValidationRule;

// ---------------------------------------------------------------------------
// Schema constructors
// ---------------------------------------------------------------------------

fn login_schema() -> FormSchema {
    FormSchema::new("login")
        .field("email", FieldType::Text)
        .with_label("email", "Email")
        .required("email")
        .with_validation("email", ValidationRule::Email)
        .field("password", FieldType::Text)
        .with_label("password", "Password")
        .required("password")
}

fn register_schema() -> FormSchema {
    FormSchema::new("register")
        .field("email", FieldType::Text)
        .with_label("email", "Email")
        .required("email")
        .with_validation("email", ValidationRule::Email)
        .field("password", FieldType::Text)
        .with_label("password", "Password")
        .required("password")
        .field("confirm", FieldType::Text)
        .with_label("confirm", "Confirm Password")
        .required("confirm")
        .field(
            "role",
            FieldType::Select {
                options: vec!["User".into(), "Admin".into(), "Viewer".into()],
            },
        )
        .with_label("role", "Role")
}

fn dynamic_schema() -> FormSchema {
    FormSchema::new("dynamic")
        .field("username", FieldType::Text)
        .with_label("username", "Username")
        .required("username")
        .with_validation(
            "username",
            ValidationRule::Regex {
                pattern: "^[a-zA-Z0-9_]{3,16}$".into(),
            },
        )
        .field("age", FieldType::Number)
        .with_label("age", "Age")
        .with_validation(
            "age",
            ValidationRule::NumberRange {
                min: Some(13.0),
                max: Some(120.0),
            },
        )
        .field("bio", FieldType::Text)
        .with_label("bio", "Bio")
        .field("subscribe", FieldType::Checkbox)
        .with_label("subscribe", "Subscribe")
}

fn nested_schema() -> FormSchema {
    FormSchema::new("nested")
        .group("profile", |s| {
            s.field("name", FieldType::Text)
                .with_label("name", "Full Name")
                .required("name")
                .field("email", FieldType::Text)
                .with_label("email", "Contact Email")
                .with_validation("email", ValidationRule::Email)
        })
        .repeatable_group("contacts", |s| {
            s.field("label", FieldType::Text)
                .with_label("label", "Label")
                .required("label")
                .field("value", FieldType::Text)
                .with_label("value", "Value")
                .with_validation("value", ValidationRule::Email)
        })
}

// ---------------------------------------------------------------------------
// Form paths (avoid repeated allocations)
// ---------------------------------------------------------------------------

fn path(segments: &[&str]) -> FormPath {
    FormPath(segments.iter().map(|s| (*s).into()).collect())
}

// ---------------------------------------------------------------------------
// Mode / mock types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum DemoMode {
    Login,
    Dynamic,
    Nested,
}

#[derive(Clone, Copy, Debug)]
enum FormKind {
    Login,
    Register,
    Dynamic,
    Nested,
}

#[derive(Clone, Debug)]
struct PendingMock {
    id: u64,
    complete_at: f64,
    form: FormKind,
    fail: bool,
}

// ---------------------------------------------------------------------------
// Public frame output
// ---------------------------------------------------------------------------

pub struct FrameOutput {
    pub batch: Batch,
    pub a11y_json: JsValue,
}

// ---------------------------------------------------------------------------
// DemoApp
// ---------------------------------------------------------------------------

pub struct DemoApp {
    ui: Ui,
    events: Vec<InputEvent>,

    // Mode & form state
    mode: DemoMode,
    auth_mode: usize,
    login_form: Form,
    register_form: Form,
    dynamic_form: Form,
    nested_form: Form,

    // State that has no auto-binding helper yet
    dynamic_bio: TextBuffer,
    dynamic_subscribe: bool,
    register_role: String,
    nested_contact_count: usize,

    // Async mock state
    pending: Vec<PendingMock>,
    status: Option<String>,
    clipboard_request: Option<String>,
}

impl DemoApp {
    pub fn new(width: f32, height: f32) -> Self {
        let theme = Theme::default_light();
        Self {
            ui: Ui::new(width, height, theme),
            events: Vec::new(),
            mode: DemoMode::Login,
            auth_mode: 0,
            login_form: Form::new(login_schema()),
            register_form: Form::new(register_schema()),
            dynamic_form: Form::new(dynamic_schema()),
            nested_form: Form::new(nested_schema()),
            dynamic_bio: TextBuffer::new(""),
            dynamic_subscribe: false,
            register_role: "User".to_string(),
            nested_contact_count: 0,
            pending: Vec::new(),
            status: None,
            clipboard_request: None,
        }
    }

    // -- Public entry point (called by WasmApp) -----------------------------

    pub fn frame(&mut self, width: f32, height: f32, scale: f32, timestamp_ms: f64) -> FrameOutput {
        self.resolve_pending(timestamp_ms);
        let events = std::mem::take(&mut self.events);
        self.ui.begin_frame(events, width, height, scale, timestamp_ms);

        // Use FormApp::build to construct the UI
        // We pass a dummy form here; the real forms live on `self`.
        self.build_all(timestamp_ms);

        let a11y = self.ui.end_frame();
        self.clipboard_request = self.ui.take_clipboard_request();
        let batch = self.ui.take_batch();
        let serializer =
            serde_wasm_bindgen::Serializer::new().serialize_large_number_types_as_bigints(true);
        let a11y_json = a11y.serialize(&serializer).unwrap_or(JsValue::NULL);
        FrameOutput { batch, a11y_json }
    }

    // -- UI construction ----------------------------------------------------

    fn build_all(&mut self, timestamp_ms: f64) {
        self.ui.label("GPU Forms UI");

        if self.ui.button("Login/Register") {
            self.mode = DemoMode::Login;
        }
        if self.ui.button("Dynamic Validation") {
            self.mode = DemoMode::Dynamic;
        }
        if self.ui.button("Nested Groups") {
            self.mode = DemoMode::Nested;
        }

        match self.mode {
            DemoMode::Login => self.build_login(timestamp_ms),
            DemoMode::Dynamic => self.build_dynamic(timestamp_ms),
            DemoMode::Nested => self.build_nested(timestamp_ms),
        }

        if let Some(status) = &self.status {
            self.ui.label(status);
        }
    }

    fn build_login(&mut self, timestamp_ms: f64) {
        let options = vec!["Login".to_string(), "Register".to_string()];
        self.ui.radio_group("Auth Mode", &options, &mut self.auth_mode);

        if self.auth_mode == 0 {
            self.ui.label("Login");
            self.ui.text_input_for(
                &mut self.login_form,
                &path(&["email"]),
                "Email",
                "email@example.com",
            );
            self.ui.text_input_masked_for(
                &mut self.login_form,
                &path(&["password"]),
                "Password",
                "password",
            );
            if self.ui.button("Submit Login") {
                self.submit_form(FormKind::Login, timestamp_ms);
            }
            self.ui.tooltip(
                "Submit Login",
                "Sends an optimistic login request with retry/backoff.",
            );
            self.show_form_status(&self.login_form.clone());
        } else {
            self.ui.label("Register");
            self.ui.text_input_for(
                &mut self.register_form,
                &path(&["email"]),
                "Email",
                "email@example.com",
            );
            self.ui.text_input_masked_for(
                &mut self.register_form,
                &path(&["password"]),
                "Password",
                "password",
            );
            self.ui.text_input_masked_for(
                &mut self.register_form,
                &path(&["confirm"]),
                "Confirm Password",
                "confirm",
            );
            let roles = vec!["User".to_string(), "Admin".to_string(), "Viewer".to_string()];
            self.ui.select("Role", &roles, &mut self.register_role);
            if self.ui.button("Submit Register") {
                // Sync the role selection (no auto-binding helper for select yet)
                let _ = self.register_form.set_value(
                    &path(&["role"]),
                    FieldValue::Selection(self.register_role.clone()),
                );
                // Cross-field validation: password confirmation
                let pw = self.register_form
                    .state()
                    .get_field(&path(&["password"]))
                    .and_then(|f| match &f.value { FieldValue::Text(s) => Some(s.clone()), _ => None })
                    .unwrap_or_default();
                let confirm = self.register_form
                    .state()
                    .get_field(&path(&["confirm"]))
                    .and_then(|f| match &f.value { FieldValue::Text(s) => Some(s.clone()), _ => None })
                    .unwrap_or_default();
                if pw != confirm {
                    self.register_form.set_field_error(
                        &path(&["confirm"]),
                        "Passwords do not match.",
                    );
                } else {
                    self.submit_form(FormKind::Register, timestamp_ms);
                }
            }
            self.show_form_status(&self.register_form.clone());
        }
    }

    fn build_dynamic(&mut self, timestamp_ms: f64) {
        self.ui.label("Dynamic Validation");
        self.ui.text_input_for(
            &mut self.dynamic_form,
            &path(&["username"]),
            "Username",
            "user",
        );
        // Age: use text_input_for, then parse to number for form validation
        self.ui.text_input_for(
            &mut self.dynamic_form,
            &path(&["age"]),
            "Age",
            "18",
        );
        // Bio: multiline has no auto-binding helper yet
        self.ui
            .text_input_multiline("Bio", &mut self.dynamic_bio, "multi-line bio", 80.0);
        self.ui.checkbox("Subscribe to updates", &mut self.dynamic_subscribe);

        if self.ui.button("Submit Profile") {
            // Sync age as number
            let age_text = self.dynamic_form
                .state()
                .get_field(&path(&["age"]))
                .and_then(|f| match &f.value { FieldValue::Text(s) => Some(s.clone()), _ => None })
                .unwrap_or_default();
            let age = age_text.parse::<f64>().unwrap_or(0.0);
            let _ = self.dynamic_form.set_value(&path(&["age"]), FieldValue::Number(age));
            // Sync fields without auto-binding
            let _ = self.dynamic_form.set_value(
                &path(&["bio"]),
                FieldValue::Text(self.dynamic_bio.text().to_string()),
            );
            let _ = self.dynamic_form.set_value(
                &path(&["subscribe"]),
                FieldValue::Bool(self.dynamic_subscribe),
            );
            self.submit_form(FormKind::Dynamic, timestamp_ms);
        }
        self.show_form_status(&self.dynamic_form.clone());
    }

    fn build_nested(&mut self, timestamp_ms: f64) {
        self.ui.label("Nested Groups");
        self.ui.text_input_for(
            &mut self.nested_form,
            &path(&["profile", "name"]),
            "Full Name",
            "Jane Doe",
        );
        self.ui.text_input_for(
            &mut self.nested_form,
            &path(&["profile", "email"]),
            "Contact Email",
            "jane@domain.com",
        );

        if self.ui.button("Add Contact") {
            let _ = self.nested_form.add_repeat_group(
                &path(&["contacts"]),
                vec![
                    FieldSchema {
                        id: "label".into(),
                        label: "Label".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Required],
                        placeholder: None,
                    },
                    FieldSchema {
                        id: "value".into(),
                        label: "Value".into(),
                        field_type: FieldType::Text,
                        rules: vec![ValidationRule::Email],
                        placeholder: None,
                    },
                ],
            );
            self.nested_contact_count += 1;
        }

        for idx in 0..self.nested_contact_count {
            self.ui.push_id(idx);
            self.ui.label(&format!("Contact {}", idx + 1));
            self.ui.text_input_for(
                &mut self.nested_form,
                &path(&["contacts", &idx.to_string(), "label"]),
                "Label",
                "Work",
            );
            self.ui.text_input_for(
                &mut self.nested_form,
                &path(&["contacts", &idx.to_string(), "value"]),
                "Email",
                "name@domain.com",
            );
            self.ui.pop_id();
        }

        if self.ui.button("Submit Nested") {
            self.submit_form(FormKind::Nested, timestamp_ms);
        }
        self.show_form_status(&self.nested_form.clone());
    }

    // -- Submission helpers -------------------------------------------------

    fn submit_form(&mut self, kind: FormKind, timestamp_ms: f64) {
        let form = self.form_mut(kind);
        let payload = serde_json::json!({ "timestamp": timestamp_ms });
        match form.start_submit(payload, 2) {
            Ok(FormEvent::SubmissionStarted(id)) => {
                self.pending.push(PendingMock {
                    id,
                    complete_at: timestamp_ms + 900.0,
                    form: kind,
                    fail: id % 2 == 0,
                });
                self.status = Some("Submitting...".to_string());
            }
            Err(FormEvent::ValidationFailed(errors)) => {
                self.status = Some(format!("Validation failed: {}", errors.len()));
            }
            _ => {}
        }
    }

    fn resolve_pending(&mut self, now: f64) {
        let mut remaining = Vec::new();
        let pending_list = std::mem::take(&mut self.pending);
        for pending in &pending_list {
            if now >= pending.complete_at {
                let form = match pending.form {
                    FormKind::Login => &mut self.login_form,
                    FormKind::Register => &mut self.register_form,
                    FormKind::Dynamic => &mut self.dynamic_form,
                    FormKind::Nested => &mut self.nested_form,
                };
                if pending.fail {
                    let _ = form.apply_error(pending.id, "Server error", true);
                    self.status = Some("Server error, rolled back.".to_string());
                } else {
                    let _ = form.apply_success(pending.id);
                    self.status = Some("Saved successfully.".to_string());
                }
            } else {
                remaining.push(pending.clone());
            }
        }
        self.pending = remaining;
    }

    fn form_mut(&mut self, kind: FormKind) -> &mut Form {
        match kind {
            FormKind::Login => &mut self.login_form,
            FormKind::Register => &mut self.register_form,
            FormKind::Dynamic => &mut self.dynamic_form,
            FormKind::Nested => &mut self.nested_form,
        }
    }

    // -- Status display -----------------------------------------------------

    fn show_form_status(&mut self, form: &Form) {
        let errors = Self::collect_errors(form);
        for error in &errors {
            let color = self.ui.theme().colors.error;
            self.ui.label_colored(error, color);
        }
        if Self::is_pending(form) {
            let color = self.ui.theme().colors.primary;
            self.ui.label_colored("Loading...", color);
        }
    }

    fn collect_errors(form: &Form) -> Vec<String> {
        form.state()
            .fields()
            .values()
            .flat_map(|field| field.errors.iter().cloned())
            .collect()
    }

    fn is_pending(form: &Form) -> bool {
        form.state().fields().values().any(|field| field.pending)
    }

    // -- Delegated accessors (used by WasmApp) ------------------------------

    pub fn take_clipboard_request(&mut self) -> Option<String> {
        self.clipboard_request.take()
    }

    pub fn set_focus(&mut self, id: u64) {
        self.ui.set_focus_by_id(id);
    }

    pub fn focused_widget_rect(&self) -> Option<[f32; 4]> {
        self.ui
            .focused_widget_rect()
            .map(|r| [r.x, r.y, r.w, r.h])
    }

    pub fn has_focused_widget(&self) -> bool {
        self.ui.focused_id().is_some()
    }

    pub fn focused_widget_kind_str(&self) -> Option<&'static str> {
        use ui_core::ui::WidgetKind;
        self.ui.focused_widget_kind().map(|k| match k {
            WidgetKind::Label => "label",
            WidgetKind::Button => "button",
            WidgetKind::Checkbox => "checkbox",
            WidgetKind::Radio => "radio",
            WidgetKind::TextInput => "textinput",
            WidgetKind::Select => "select",
            WidgetKind::Group => "group",
            _ => "unknown",
        })
    }

    // -- Event forwarding ---------------------------------------------------

    pub fn handle_pointer_down(
        &mut self, x: f32, y: f32, button: u16,
        ctrl: bool, alt: bool, shift: bool, meta: bool,
    ) {
        self.events.push(InputEvent::PointerDown(PointerEvent {
            pos: ui_core::types::Vec2::new(x, y),
            button: Some(map_button(button)),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        }));
    }

    pub fn handle_pointer_up(
        &mut self, x: f32, y: f32, button: u16,
        ctrl: bool, alt: bool, shift: bool, meta: bool,
    ) {
        self.events.push(InputEvent::PointerUp(PointerEvent {
            pos: ui_core::types::Vec2::new(x, y),
            button: Some(map_button(button)),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        }));
    }

    pub fn handle_pointer_move(
        &mut self, x: f32, y: f32,
        ctrl: bool, alt: bool, shift: bool, meta: bool,
    ) {
        self.events.push(InputEvent::PointerMove(PointerEvent {
            pos: ui_core::types::Vec2::new(x, y),
            button: None,
            modifiers: Modifiers { ctrl, alt, shift, meta },
        }));
    }

    pub fn handle_wheel(
        &mut self, x: f32, y: f32, dx: f32, dy: f32,
        ctrl: bool, alt: bool, shift: bool, meta: bool,
    ) {
        self.events.push(InputEvent::PointerWheel {
            pos: ui_core::types::Vec2::new(x, y),
            delta: ui_core::types::Vec2::new(dx, dy),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        });
    }

    pub fn handle_key_down(
        &mut self, code: &str,
        ctrl: bool, alt: bool, shift: bool, meta: bool,
    ) {
        self.events.push(InputEvent::KeyDown {
            code: KeyCode::from_code_str(code),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        });
    }

    pub fn handle_key_up(
        &mut self, code: &str,
        ctrl: bool, alt: bool, shift: bool, meta: bool,
    ) {
        self.events.push(InputEvent::KeyUp {
            code: KeyCode::from_code_str(code),
            modifiers: Modifiers { ctrl, alt, shift, meta },
        });
    }

    pub fn handle_text_input(&mut self, text: String) {
        self.events.push(InputEvent::TextInput(TextInputEvent { text }));
    }

    pub fn handle_composition_start(&mut self) {
        self.events.push(InputEvent::CompositionStart);
    }

    pub fn handle_composition_update(&mut self, text: String) {
        self.events.push(InputEvent::CompositionUpdate(text));
    }

    pub fn handle_composition_end(&mut self, text: String) {
        self.events.push(InputEvent::CompositionEnd(text));
    }

    pub fn handle_paste(&mut self, text: String) {
        self.events.push(InputEvent::Paste(text));
    }
}

// ---------------------------------------------------------------------------
// FormApp trait implementation
// ---------------------------------------------------------------------------

impl FormApp for DemoApp {
    fn schema(&self) -> FormSchema {
        // The demo hosts multiple forms; expose the login schema as the
        // primary one for trait compliance. In a real single-form app this
        // would be the only schema.
        login_schema()
    }

    fn build(&mut self, ui: &mut Ui, form: &mut Form) {
        // Single-form usage example: render a minimal login form using the
        // trait-provided form. The full demo uses `build_all` with multiple
        // forms, but this shows the canonical pattern.
        ui.label("Login (FormApp)");
        ui.text_input_for(form, &path(&["email"]), "Email", "email@example.com");
        ui.text_input_masked_for(form, &path(&["password"]), "Password", "password");
        if ui.button("Submit") {
            let _ = self.on_submit(form);
        }
    }

    fn on_submit(&mut self, form: &Form) -> Result<(), String> {
        let errors = Self::collect_errors(form);
        if errors.is_empty() {
            self.status = Some("Submitted via FormApp!".to_string());
            Ok(())
        } else {
            Err(format!("Validation failed: {}", errors.len()))
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn map_button(button: u16) -> PointerButton {
    match button {
        0 => PointerButton::Left,
        1 => PointerButton::Middle,
        2 => PointerButton::Right,
        other => PointerButton::Other(other),
    }
}

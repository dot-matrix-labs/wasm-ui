//! `wham-elements` — HTML spec element layer for the wham GPU-rendered forms library.
//!
//! This crate provides the form model, text editing, validation rules, and
//! accessibility tree for building accessible GPU-rendered form UIs. It also
//! provides higher-level element abstractions (buttons, links, text nodes) built
//! on top of the `wham-core` primitives. It depends only on `wham-core` and has
//! no dependency on browser APIs; it can be compiled and tested with `cargo test`
//! on any host platform.
//!
//! # Modules
//!
//! - [`accessibility`] — Accessibility tree: ARIA roles, states, and the node hierarchy.
//! - [`button`] — Button element with ARIA role `button` and keyboard interaction.
//! - [`form`] — Form model: schema, field values, validation, and submission lifecycle.
//! - [`icon`] — Icon pack loading and UV coordinate lookup.
//! - [`link`] — Link element with ARIA role `link` and keyboard interaction.
//! - [`text`] — Grapheme-aware text buffer with caret, selection, IME, and undo/redo.
//! - [`text_node`] — Static text node element with semantic variant.
//! - [`validation`] — Field-level and cross-field validation rules.

/// ARIA role: none (structural) — accessibility tree: roles, states, and the hidden DOM mirror interface.
pub mod accessibility;
/// ARIA role: `button` — interactive control that triggers an action.
pub mod button;
/// ARIA role: form — form model: schema, field values, validation, and submission lifecycle.
pub mod form;
/// ARIA role: img — icon pack loading and UV coordinate lookup.
pub mod icon;
/// ARIA role: `link` — navigational hyperlink element.
pub mod link;
/// ARIA role: textbox — grapheme-aware text buffer with caret, selection, IME, and undo/redo.
pub mod text;
/// ARIA role: `heading` or implicit — static text node with semantic variant.
pub mod text_node;
/// ARIA role: none (structural) — validation rules and error types for form fields.
pub mod validation;

pub use accessibility::{A11yNode, A11yNodeEl, A11yRole, A11yState, A11yTree, A11yTreeEl};
pub use button::{Button, ButtonKind, ButtonState};
pub use form::{
    AutocompleteHint, FieldId, FieldSchema, FieldState, FieldType, FieldValue, Form, FormEvent,
    FormPath, FormSchema, FormState, PendingSubmission,
};
pub use icon::{IconEntry, IconId, IconPack};
pub use link::{Link, LinkState};
pub use text::{Caret, Selection, TextBuffer, TextEditOp};
pub use text_node::{TextNode, TextVariant};
pub use validation::{ValidationError, ValidationRule};

//! Browser helpers (isolated profiles, tab ownership guard, and Passkey wall detection). The
//! CDP browser itself is `screen::protected_browser`.

pub mod passkey_detector;
pub mod profile_manager;
pub mod proxy;
pub mod tab_guard;

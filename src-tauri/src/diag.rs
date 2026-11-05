//! Native event and UI effects for diagnostic messages resolved by Jac.

use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub struct ErrorDto {
    pub headline: String,
    pub detail: String,
}

static LAST_ERROR: OnceLock<Mutex<Option<(Option<String>, ErrorDto)>>> = OnceLock::new();

fn platform() -> &'static str {
    #[cfg(target_os = "macos")]
    { "macos" }
    #[cfg(target_os = "windows")]
    { "windows" }
    #[cfg(target_os = "linux")]
    { "linux" }
}

/// Report a stable failure code; Jac selects the user-facing copy and timing.
#[allow(dead_code)]
pub fn report(app: &AppHandle, code: &'static str) {
    let _ = app.emit("openflow://diagnostic/request", serde_json::json!({
        "code": code,
        "platform": platform(),
    }));
}

/// Forward runtime-provided text through the same Jac presentation policy.
#[allow(dead_code)]
pub fn report_text(app: &AppHandle, headline: String, detail: String) {
    eprintln!("[openflow] ⚠ {headline}: {detail}");
    let _ = app.emit("openflow://diagnostic/request", serde_json::json!({
        "code": "runtime_message",
        "platform": platform(),
        "headline": headline,
        "detail": detail,
    }));
}

/// Complete the Jac policy request with native window/event side effects.
pub fn present(app: &AppHandle, code: String, headline: String, detail: String) {
    let dto = ErrorDto { headline, detail };
    *LAST_ERROR.get_or_init(|| Mutex::new(None)).lock().unwrap() =
        Some((Some(code), dto.clone()));
    crate::emit_flowbar_state(app, "error");
    let _ = app.emit("openflow://error", dto);
}

/// Return the latest error for the Hub banner.
pub fn last_error() -> Option<ErrorDto> {
    LAST_ERROR
        .get()
        .and_then(|slot| slot.lock().unwrap().as_ref().map(|(_, dto)| dto.clone()))
}

pub fn clear_last_error() {
    if let Some(slot) = LAST_ERROR.get() {
        *slot.lock().unwrap() = None;
    }
}

pub fn clear_if(code: &'static str) {
    if let Some(slot) = LAST_ERROR.get() {
        let mut slot = slot.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|(last_code, _)| last_code.as_deref() == Some(code))
        {
            *slot = None;
        }
    }
}

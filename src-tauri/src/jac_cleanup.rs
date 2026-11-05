//! Tauri event transport for the Jac-owned cleanup policy.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use openflow_core::{CleanupContext, Settings};

pub fn native_messages(
    messages: Vec<(String, String)>,
) -> Result<Vec<openflow_core::cleanup::CleanupMsg>, String> {
    messages
        .into_iter()
        .map(|(role, content)| {
            let role = match role.as_str() {
                "system" => "system",
                "user" => "user",
                "assistant" => "assistant",
                _ => return Err("Jac supplied an unsupported cleanup message role".into()),
            };
            Ok(openflow_core::cleanup::CleanupMsg { role, content })
        })
        .collect()
}

static READY: AtomicBool = AtomicBool::new(false);
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
static RESPONSES: OnceLock<Mutex<HashMap<String, mpsc::Sender<String>>>> = OnceLock::new();

#[derive(Clone, Serialize)]
struct CleanupRequest {
    request_id: String,
    transcript: String,
    raw_fallback: String,
    cloud_asr_response: bool,
    cleanup_mode: String,
    openai_model: String,
    anthropic_model: String,
    cleanup_level: String,
    dictionary: Vec<crate::hotkey::DictEntryDto>,
    app_bundle_id: String,
    window_context: String,
}

pub fn register() {
    READY.store(true, Ordering::Release);
}

pub fn is_ready() -> bool {
    READY.load(Ordering::Acquire)
}

pub fn unregister() {
    READY.store(false, Ordering::Release);
}

pub fn resolve(request_id: &str, text: String) -> bool {
    RESPONSES
        .get()
        .and_then(|responses| responses.lock().ok())
        .and_then(|mut responses| responses.remove(request_id))
        .is_some_and(|sender| sender.send(text).is_ok())
}

pub fn request(
    app: &AppHandle,
    transcript: &str,
    raw_fallback: &str,
    cloud_asr_response: bool,
    settings: &Settings,
    context: &CleanupContext,
    dictionary: &[crate::hotkey::DictEntryDto],
) -> Option<String> {
    if !READY.load(Ordering::Acquire) {
        return None;
    }
    let request_id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed).to_string();
    let (sender, receiver) = mpsc::channel();
    RESPONSES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()?
        .insert(request_id.clone(), sender);

    let payload = CleanupRequest {
        request_id: request_id.clone(),
        transcript: transcript.to_string(),
        raw_fallback: raw_fallback.to_string(),
        cloud_asr_response,
        cleanup_mode: settings.cleanup_mode.clone(),
        openai_model: settings.openai_model.clone(),
        anthropic_model: settings.anthropic_model.clone(),
        cleanup_level: settings.cleanup_level.clone(),
        dictionary: dictionary.to_vec(),
        app_bundle_id: context.app_bundle_id.clone().unwrap_or_default(),
        window_context: context.window_context.clone().unwrap_or_default(),
    };
    if app
        .emit_to("main", "openflow://cleanup/request", payload)
        .is_err()
    {
        if let Some(responses) = RESPONSES.get() {
            if let Ok(mut responses) = responses.lock() {
                responses.remove(&request_id);
            }
        }
        return None;
    }

    let result = receiver.recv_timeout(Duration::from_secs(45)).ok();
    if let Some(responses) = RESPONSES.get() {
        if let Ok(mut responses) = responses.lock() {
            responses.remove(&request_id);
        }
    }
    result
}


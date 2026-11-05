//! Event transport for Jac's local dictation audio quality policy.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

static READY: AtomicBool = AtomicBool::new(false);
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
static RESPONSES: OnceLock<Mutex<HashMap<String, mpsc::Sender<AudioDecision>>>> = OnceLock::new();

#[derive(Clone, Copy)]
pub struct AudioDecision {
    pub accepted: bool,
    pub real_attempt: bool,
}

#[derive(Clone, Serialize)]
struct AudioPolicyRequest {
    request_id: String,
    peak: f32,
    rms: f32,
    duration_secs: f32,
}

pub fn register() {
    READY.store(true, Ordering::Release);
}
pub fn unregister() {
    READY.store(false, Ordering::Release);
}

pub fn resolve(request_id: &str, accepted: bool, real_attempt: bool) -> bool {
    RESPONSES
        .get()
        .and_then(|responses| responses.lock().ok())
        .and_then(|mut responses| responses.remove(request_id))
        .is_some_and(|sender| {
            sender
                .send(AudioDecision {
                    accepted,
                    real_attempt,
                })
                .is_ok()
        })
}

pub fn assess(app: &AppHandle, peak: f32, rms: f32, duration_secs: f32) -> Option<AudioDecision> {
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
    let payload = AudioPolicyRequest {
        request_id: request_id.clone(),
        peak,
        rms,
        duration_secs,
    };
    if app
        .emit_to("main", "openflow://audio/policy_request", payload)
        .is_err()
    {
        if let Some(responses) = RESPONSES.get() {
            if let Ok(mut map) = responses.lock() {
                map.remove(&request_id);
            }
        }
        return None;
    }
    let result = receiver.recv_timeout(Duration::from_secs(2)).ok();
    if let Some(responses) = RESPONSES.get() {
        if let Ok(mut map) = responses.lock() {
            map.remove(&request_id);
        }
    }
    result
}

//! Request completed-dictation record shaping from the Jac stats policy.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};

static READY: AtomicBool = AtomicBool::new(false);
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);
static RESPONSES: OnceLock<Mutex<HashMap<String, mpsc::Sender<Option<Value>>>>> = OnceLock::new();

#[derive(Clone, Serialize)]
struct RecordRequest {
    request_id: String,
    text: String,
    duration_secs: f32,
    ts_unix: u64,
    app: Option<String>,
}

pub fn register() { READY.store(true, Ordering::Release); }
pub fn unregister() { READY.store(false, Ordering::Release); }

pub fn resolve(request_id: &str, record: Option<Value>) -> bool {
    RESPONSES
        .get()
        .and_then(|map| map.lock().ok())
        .and_then(|mut map| map.remove(request_id))
        .is_some_and(|sender| sender.send(record).is_ok())
}

pub fn request(
    app: &AppHandle,
    text: &str,
    duration_secs: f32,
    ts_unix: u64,
    target_app: Option<String>,
) -> Option<Value> {
    // A dictation can finish while the Hub is still mounting. Wait within the
    // same bounded request budget so Jac remains the only record shaper without
    // dropping a session just because its listener registered a moment later.
    let deadline = Instant::now() + Duration::from_secs(3);
    while !READY.load(Ordering::Acquire) {
        if Instant::now() >= deadline { return None; }
        std::thread::sleep(Duration::from_millis(10));
    }
    let request_id = NEXT_REQUEST.fetch_add(1, Ordering::Relaxed).to_string();
    let (sender, receiver) = mpsc::channel();
    RESPONSES.get_or_init(|| Mutex::new(HashMap::new())).lock().ok()?.insert(request_id.clone(), sender);
    let payload = RecordRequest {
        request_id: request_id.clone(), text: text.to_string(), duration_secs, ts_unix, app: target_app,
    };
    if app.emit_to("main", "openflow://stats/record", payload).is_err() {
        if let Some(map) = RESPONSES.get() { if let Ok(mut map) = map.lock() { map.remove(&request_id); } }
        return None;
    }
    let record = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())).ok().flatten();
    if let Some(map) = RESPONSES.get() { if let Ok(mut map) = map.lock() { map.remove(&request_id); } }
    record
}

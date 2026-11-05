//! Linux platform layer for OpenFlow: a global keyboard listener for
//! push-to-talk, clipboard+key-simulation text injection, and the same dictation
//! pipeline (audio → Whisper ASR → cleanup LLM → paste) that macOS/Windows use,
//! plus Hub-facing settings/stats/dictionary functions.
//!
//! Default push-to-talk key: Right Ctrl (same as Windows).
//! Keyboard listening uses `rdev` (X11 + Wayland). Paste uses `arboard` + `enigo`.

#![cfg(target_os = "linux")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use openflow_core::{AsrEngine, CleanupContext};

const OVERLAY_LABEL: &str = "openflow_bar";

static APP: OnceLock<AppHandle> = OnceLock::new();
static CLOCK: OnceLock<Instant> = OnceLock::new();
static RECORDING: AtomicBool = AtomicBool::new(false);
static JAC_DICTATION_READY: AtomicBool = AtomicBool::new(false);
static JAC_TICKER_RUNNING: AtomicBool = AtomicBool::new(false);
static PENDING_DICTATION_INPUTS: OnceLock<Mutex<VecDeque<serde_json::Value>>> = OnceLock::new();
static CAPTURE: OnceLock<Mutex<Option<openflow_audio::CaptureHandle>>> = OnceLock::new();
static ASR: OnceLock<Arc<openflow_asr::WhisperEngine>> = OnceLock::new();
static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
static OPENAI: OnceLock<Mutex<Option<openflow_cleanup::OpenAiProvider>>> = OnceLock::new();
static ANTHROPIC: OnceLock<Mutex<Option<openflow_cleanup::AnthropicProvider>>> = OnceLock::new();
static SETTINGS: OnceLock<Mutex<openflow_core::Settings>> = OnceLock::new();
static DICTIONARY: OnceLock<Mutex<Vec<crate::hotkey::DictEntryDto>>> = OnceLock::new();
static STATS: OnceLock<Mutex<Vec<serde_json::Value>>> = OnceLock::new();

// ── Support paths (XDG-compliant) ──────────────────────────────────────────────

fn support_dir() -> std::path::PathBuf {
    // XDG_DATA_HOME or ~/.local/share
    if let Ok(d) = std::env::var("XDG_DATA_HOME") {
        if !d.is_empty() {
            return std::path::PathBuf::from(d).join("OpenFlow");
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(home).join(".local/share/OpenFlow")
}

fn settings_path() -> std::path::PathBuf {
    support_dir().join("settings.json")
}
fn dict_path() -> std::path::PathBuf {
    support_dir().join("dictionary.json")
}
fn stats_path() -> std::path::PathBuf {
    support_dir().join("stats.json")
}
fn whisper_model_path() -> std::path::PathBuf {
    let dir = support_dir().join("models");
    for name in [
        "ggml-large-v3-turbo.bin",
        "ggml-medium.en.bin",
        "ggml-small.en.bin",
        "ggml-base.en.bin",
    ] {
        let p = dir.join(name);
        if p.exists() {
            return p;
        }
    }
    dir.join("ggml-base.en.bin")
}

// ── Time helpers ────────────────────────────────────────────────────────────────

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    CLOCK.get().map(|c| c.elapsed().as_millis() as u64).unwrap_or(0)
}

// ── Pill state emission ─────────────────────────────────────────────────────────

fn emit_bar(state: &'static str) {
    if let Some(app) = APP.get() {
        #[derive(Clone, serde::Serialize)]
        struct P {
            state: &'static str,
        }
        let _ = app.emit_to(OVERLAY_LABEL, "openflow://flowbar/state", P { state });
    }
}

fn send_jac_dictation_input(input: serde_json::Value) -> bool {
    let Some(app) = APP.get() else { return false; };
    let mut pending = PENDING_DICTATION_INPUTS.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap();
    let is_tick = input.get("kind").and_then(|v| v.as_str()) == Some("tick");
    if !JAC_DICTATION_READY.load(Ordering::Acquire) {
        if !is_tick && pending.len() < 128 { pending.push_back(input); }
        return false;
    }
    if pending.len() >= 128 {
        if is_tick { return false; }
        pending.pop_front();
    }
    pending.push_back(input);
    while let Some(next) = pending.pop_front() {
        if app.emit_to("main", "openflow://dictation/input", next.clone()).is_err() {
            pending.push_front(next);
            return false;
        }
    }
    true
}

fn flush_dictation_inputs() {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":"tick", "at_ms":now_ms()}));
}

pub fn register_jac_dictation() {
    JAC_DICTATION_READY.store(true, Ordering::SeqCst);
    flush_dictation_inputs();
    if JAC_TICKER_RUNNING.swap(true, Ordering::SeqCst) { return; }
    std::thread::spawn(|| {
        while JAC_DICTATION_READY.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
            let _ = send_jac_dictation_input(serde_json::json!({"kind":"tick", "at_ms":now_ms()}));
        }
        JAC_TICKER_RUNNING.store(false, Ordering::SeqCst);
    });
}

pub fn unregister_jac_dictation() { JAC_DICTATION_READY.store(false, Ordering::SeqCst); }

pub fn apply_jac_dictation_actions(actions: Vec<serde_json::Value>) {
    for action in actions {
        let kind = action.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
        let session = action.get("session").and_then(|v| v.as_u64()).unwrap_or_default();
        match kind {
            "start_capture" => on_ptt_down(),
            "stop_capture_and_finalize" => on_ptt_up(session),
            "discard_capture" => discard_capture(),
            "show_bar" => emit_bar(match action.get("state").and_then(|v| v.as_str()).unwrap_or("idle") {
                "recording" => "recording", "locked" => "locked", "transcribing" => "transcribing",
                "done" => "done", "cancelled" => "cancelled", _ => "idle"
            }),
            "warn_session_cap" => emit_bar("session_cap_warning"),
            "play_ping" | "run_pipeline" => {},
            _ => {}
        }
    }
}

fn discard_capture() {
    RECORDING.store(false, Ordering::SeqCst);
    if let Some(slot) = CAPTURE.get() { let _ = slot.lock().unwrap().take(); }
}

fn pipeline_finished(session: Option<u64>, committed: bool) {
    if let Some(session) = session {
        let kind = if committed { "pipeline_committed" } else { "pipeline_failed" };
        let _ = send_jac_dictation_input(serde_json::json!({"kind":kind, "session":session, "at_ms":now_ms()}));
    }
}

// ── Frontmost app detection (Linux) ─────────────────────────────────────────────

/// Try to detect the frontmost window's class/name. Returns None on failure
/// or when running under an unsupported display server.
fn foreground_app() -> Option<String> {
    // Try xdotool first (most reliable on X11)
    if let Ok(out) = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowclassname"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    // Fallback: try to get the window title via xprop
    if let Ok(out) = std::process::Command::new("sh")
        .arg("-c")
        .arg("xprop -id $(xprop -root 32x '\t$0' _NET_ACTIVE_WINDOW | cut -f2) WM_CLASS 2>/dev/null | cut -d'\"' -f4")
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

// ── Text injection: clipboard + Ctrl+V via enigo ────────────────────────────────

pub fn paste_text(text: &str) -> anyhow::Result<()> {
    use arboard::Clipboard;
    let mut cb = Clipboard::new()?;
    let saved = cb.get_text().ok();
    cb.set_text(text.to_string())?;
    std::thread::sleep(Duration::from_millis(60));

    // Simulate Ctrl+V. Try ydotool first (Wayland), then xdotool (X11).
    let ydotool = std::process::Command::new("ydotool")
        .args(["key", "29:1", "47:1", "47:0", "29:0"]) // Ctrl down, V down, V up, Ctrl up
        .output();
    if ydotool.is_err() || !ydotool.as_ref().map(|o| o.status.success()).unwrap_or(false) {
        // Fallback: xdotool for X11
        let _ = std::process::Command::new("xdotool")
            .args(["key", "--clearmodifiers", "ctrl+v"])
            .output();
    }

    std::thread::sleep(Duration::from_millis(150));
    if let Some(prev) = saved {
        let _ = cb.set_text(prev);
    }
    Ok(())
}

// ── Cleanup (shared, cross-platform building blocks) ────────────────────────────

fn current_settings_inner() -> openflow_core::Settings {
    SETTINGS
        .get()
        .map(|m| m.lock().unwrap().clone())
        .unwrap_or_default()
}

fn read_key(account: &str, env_var: &str) -> Option<String> {
    if let Ok(k) = std::env::var(env_var) {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Some(k);
        }
    }
    keyring::Entry::new("com.openflow.openflow", account)
        .ok()
        .and_then(|e| e.get_password().ok())
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

fn clean_transcript(raw: &str, cloud_asr_response: bool) -> String {
    let settings = current_settings_inner();
    let dictionary = DICTIONARY
        .get()
        .map(|d| d.lock().unwrap().clone())
        .unwrap_or_default();
    let ctx = CleanupContext {
        app_bundle_id: foreground_app(),
        ..Default::default()
    };
    if let Some(app) = APP.get() {
        if let Some(cleaned) = crate::jac_cleanup::request(app, raw, raw, cloud_asr_response, &settings, &ctx, &dictionary) {
            return cleaned;
        }
    }
    if cloud_asr_response { String::new() } else { raw.to_string() }
}

pub fn cleanup_with_messages(
    _app: &AppHandle,
    provider: &str,
    raw: &str,
    max_tokens: usize,
    messages: Vec<(String, String)>,
    request_body: Option<serde_json::Value>,
) -> Result<String, String> {
    let native_messages = crate::jac_cleanup::native_messages(messages)?;
    let request_body = request_body.as_ref();
    let result = match provider {
        "open_ai" => request_body.and_then(|body| OPENAI.get().and_then(|slot| {
            slot.lock()
                .ok()?
                .as_ref()
                .map(|item| item.cleanup_with_request_body(body))
        })),
        "anthropic" => request_body.and_then(|body| ANTHROPIC.get().and_then(|slot| {
            slot.lock()
                .ok()?
                .as_ref()
                .map(|item| item.cleanup_with_request_body(body))
        })),
        "local" => LOCAL.get().and_then(|slot| {
            slot.lock()
                .ok()?
                .as_mut()
                .map(|item| item.cleanup_with_budget(&native_messages, max_tokens))
        }),
        "raw" => return Ok(raw.to_string()),
        _ => None,
    };
    result
        .ok_or_else(|| format!("Cleanup provider {provider} is not configured"))?
        .map_err(|error| error.to_string())
}

fn record_dictation(text: &str, duration_secs: f32, app: Option<String>) {
    let record = APP.get().and_then(|handle| {
        crate::jac_stats::request(handle, text, duration_secs, unix_now(), app.clone())
    });
    let Some(record) = record else {
        eprintln!("[openflow:linux] Jac stats bridge unavailable; dictation record was not persisted");
        return;
    };
    if let Some(m) = STATS.get() {
        let mut store = m.lock().unwrap();
        store.push(record);
        let _ = crate::stats_store::save(&stats_path(), &store);
    }
}

// ── The push-to-talk pipeline ───────────────────────────────────────────────────

fn on_ptt_down() {
    if RECORDING.swap(true, Ordering::SeqCst) {
        return; // already recording
    }
    let _ = now_ms();
    eprintln!("[openflow:linux] PTT DOWN — starting capture");
    std::thread::spawn(|| match openflow_audio::start(|_: &[f32]| {}) {
        Ok(handle) => {
            *CAPTURE.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(handle);
        }
        Err(e) => eprintln!("[openflow:linux] mic capture failed: {e}"),
    });
}

fn on_ptt_up(session: u64) {
    if !RECORDING.swap(false, Ordering::SeqCst) {
        pipeline_finished(Some(session), false);
        return; // wasn't recording
    }
    eprintln!("[openflow:linux] PTT UP — finalizing");
    let app = foreground_app();
    let handle = CAPTURE.get().and_then(|slot| slot.lock().unwrap().take());
    std::thread::spawn(move || {
        let committed = (|| -> bool {
        let Some(res) = handle.and_then(|h| h.stop()) else {
            eprintln!("[openflow:linux] no audio captured");
            return false;
        };
        let Some(asr) = ASR.get().cloned() else {
            eprintln!("[openflow:linux] ASR not ready");
            return false;
        };
        let pcm = openflow_audio::resample_to_16k(&res.samples, res.sample_rate);
        let mut committed = false;
        match asr.transcribe(&pcm) {
            Ok(t) => {
                let raw = &t.text;
                eprintln!("[openflow:linux] TRANSCRIPT: \"{}\"", raw);
                let text = clean_transcript(raw, false);
                if text != *raw {
                    eprintln!("[openflow:linux] CLEANED:   \"{}\"", text);
                }
                if !text.is_empty() {
                    if let Err(e) = paste_text(&text) {
                        eprintln!("[openflow:linux] paste failed: {e}");
                    } else {
                        committed = true;
                    }
                    record_dictation(&text, res.duration_secs(), app);
                }
            }
            Err(e) => eprintln!("[openflow:linux] ASR error: {e}"),
        }
        committed
        })();
        pipeline_finished(Some(session), committed);
    });
}

fn route_ptt_input(kind: &str) {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":kind, "binding":"push_to_talk", "at_ms":now_ms()}));
}

// ── Global keyboard listener via rdev ────────────────────────────────────────────

/// Parse a string like "ControlRight" into the corresponding rdev::Key.
/// Returns None for unrecognized keys.
fn parse_ptt_key(s: &str) -> Option<rdev::Key> {
    use rdev::Key::*;
    match s {
        "ControlRight" => Some(ControlRight),
        "ControlLeft" => Some(ControlLeft),
        "AltRight" => Some(AltGr),
        "AltLeft" => Some(Alt),
        "ShiftRight" => Some(ShiftRight),
        "ShiftLeft" => Some(ShiftLeft),
        "MetaRight" => Some(MetaRight),
        "MetaLeft" => Some(MetaLeft),
        "CapsLock" => Some(CapsLock),
        "F1" => Some(F1),
        "F2" => Some(F2),
        "F3" => Some(F3),
        "F4" => Some(F4),
        "F5" => Some(F5),
        "F6" => Some(F6),
        "F7" => Some(F7),
        "F8" => Some(F8),
        "F9" => Some(F9),
        "F10" => Some(F10),
        "F11" => Some(F11),
        "F12" => Some(F12),
        _ => {
            eprintln!("[openflow:linux] unrecognized push-to-talk key '{s}', falling back to ControlRight");
            Some(ControlRight)
        }
    }
}

/// Human-readable label for a key variant string (shown in the startup message).
fn ptt_key_label(s: &str) -> &str {
    match s {
        "ControlRight" => "Right Ctrl",
        "ControlLeft" => "Left Ctrl",
        "AltRight" => "Right Alt",
        "AltLeft" => "Left Alt",
        "ShiftRight" => "Right Shift",
        "ShiftLeft" => "Left Shift",
        "MetaRight" => "Right Super/Meta",
        "MetaLeft" => "Left Super/Meta",
        "CapsLock" => "Caps Lock",
        s if s.starts_with('F') => s, // F1-F12
        _ => s,
    }
}

fn spawn_keyboard_listener() {
    // Capture the configured key *now* (before the thread starts) so the
    // listener always sees the key that was active at install time.
    let ptt_key_str = current_settings_inner().push_to_talk_key;
    let ptt_key = parse_ptt_key(&ptt_key_str);
    let label = ptt_key_label(&ptt_key_str).to_string();

    std::thread::spawn(move || {
        let Some(ptt_key) = ptt_key else {
            eprintln!("[openflow:linux] invalid push-to-talk key '{ptt_key_str}' — listener not started");
            return;
        };
        // Give the app a moment to fully initialize before hooking keys
        std::thread::sleep(Duration::from_millis(500));
        eprintln!("[openflow:linux] starting global keyboard listener (push-to-talk: {label})");

        if let Err(e) = rdev::listen(move |event| {
            use rdev::EventType::{KeyPress, KeyRelease};
            match event.event_type {
                KeyPress(k) if k == ptt_key => route_ptt_input("trigger_down"),
                KeyRelease(k) if k == ptt_key => route_ptt_input("trigger_up"),
                _ => {}
            }
        }) {
            eprintln!(
                "[openflow:linux] rdev keyboard listener failed: {e:?}. \
                 You may need to add yourself to the 'input' group: \
                 sudo usermod -aG input $USER && reboot"
            );
        }
    });
}

// ── Public surface (mirrors the macOS `hotkey::` and Windows `win::` functions) ──

pub fn install(app: AppHandle) {
    let _ = APP.set(app.clone());
    let _ = CLOCK.set(Instant::now());
    let _ = SETTINGS.set(Mutex::new(openflow_core::Settings::load(&settings_path())));
    let _ = DICTIONARY.set(Mutex::new(crate::hotkey::load_dictionary_entries(&dict_path())));
    let _ = STATS.set(Mutex::new(crate::stats_store::load(&stats_path())));
    let _ = OPENAI.set(Mutex::new(None));
    let _ = ANTHROPIC.set(Mutex::new(None));
    let _ = LOCAL.set(Mutex::new(None));

    // Ensure support dirs exist
    let _ = std::fs::create_dir_all(&support_dir());
    let _ = std::fs::create_dir_all(&support_dir().join("models"));

    rebuild_providers();
    eprintln!(
        "[openflow:linux] data dir: {}",
        support_dir().display()
    );

    // Load Whisper ASR model
    std::thread::spawn(|| {
        let path = whisper_model_path();
        if !path.exists() {
            eprintln!(
                "[openflow:linux] ⚠ ASR model not found at {}. \
                 Download a whisper.cpp model (e.g., ggml-base.en.bin) and place it there.",
                path.display()
            );
            return;
        }
        match openflow_asr::WhisperEngine::load(&path) {
            Ok(engine) => {
                let _ = ASR.set(Arc::new(engine));
                eprintln!("[openflow:linux] ASR model loaded — ready to transcribe");
            }
            Err(e) => eprintln!("[openflow:linux] ASR model load failed: {e}"),
        }
    });

    // Start the local cleanup worker
    std::thread::spawn(|| {
        if let Some(w) = crate::local_llm::spawn_default() {
            if let Some(slot) = LOCAL.get() {
                *slot.lock().unwrap() = Some(w);
            }
        }
    });

    // Start the global keyboard listener for push-to-talk
    let settings = current_settings_inner();
    let ptt_label = ptt_key_label(&settings.push_to_talk_key);
    spawn_keyboard_listener();

    eprintln!("[openflow:linux] installed — hold {ptt_label} to dictate");
}

pub fn stop_dictation() {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":"stop", "at_ms":now_ms()}));
}

pub fn cancel_dictation() {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":"cancel", "at_ms":now_ms()}));
}

pub fn trigger_hands_free() {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":"trigger_down", "binding":"hands_free", "at_ms":now_ms()}));
}

pub fn current_settings() -> openflow_core::Settings {
    current_settings_inner()
}

pub fn update_settings(new: openflow_core::Settings) {
    if let Some(m) = SETTINGS.get() {
        *m.lock().unwrap() = new.clone();
    }
    let _ = new.save(&settings_path());
    rebuild_providers();
}

pub fn rebuild_providers() {
    let settings = current_settings_inner();

    let openai = read_key("openai_api_key", "OPENAI_API_KEY").map(|k| {
        openflow_cleanup::OpenAiProvider::with_base_url(
            k,
            Some(settings.openai_base_url.clone()),
        )
    });
    let anthropic = read_key("anthropic_api_key", "ANTHROPIC_API_KEY")
        .map(openflow_cleanup::AnthropicProvider::new);

    match OPENAI.get() {
        Some(m) => *m.lock().unwrap() = openai,
        None => {
            let _ = OPENAI.set(Mutex::new(openai));
        }
    }
    match ANTHROPIC.get() {
        Some(m) => *m.lock().unwrap() = anthropic,
        None => {
            let _ = ANTHROPIC.set(Mutex::new(anthropic));
        }
    }
}

pub fn stats_records() -> Vec<serde_json::Value> {
    STATS
        .get()
        .map(|m| m.lock().unwrap().clone())
        .unwrap_or_default()
}

pub fn dictionary_entries() -> Vec<crate::hotkey::DictEntryDto> {
    DICTIONARY
        .get()
        .map(|m| m.lock().unwrap().clone())
        .unwrap_or_default()
}

pub fn dictionary_replace(entries: Vec<crate::hotkey::DictEntryDto>) {
    if let Some(m) = DICTIONARY.get() {
        let mut stored = m.lock().unwrap();
        *stored = entries;
        let _ = crate::hotkey::save_dictionary_entries(&dict_path(), &stored);
    }
}

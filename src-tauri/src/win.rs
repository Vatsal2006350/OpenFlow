//! Windows platform layer for OpenFlow: a low-level keyboard hook for
//! push-to-talk, clipboard+SendInput text injection, and foreground-app detection,
//! plus the same dictation pipeline (audio → Whisper ASR → cleanup LLM → paste) and
//! the Hub-facing settings/stats/dictionary functions the Tauri commands call.
//!
//! ⚠️ UNVERIFIED: this module was written on macOS and has **never been compiled or
//! run on Windows**. The shared crates (audio, ASR, cleanup, core) are
//! cross-platform, but this Win32 glue will almost certainly need fixes before it
//! builds and runs. It is `cfg(target_os = "windows")` so it does not affect — and
//! is not checked by — the macOS build. Treat it as a starting point, not a
//! shipping port. Default push-to-talk key: Right Ctrl.

#![cfg(target_os = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL,
    VK_RCONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW,
    HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use openflow_core::{AsrEngine, CleanupContext};

/// Push-to-talk key. Right Ctrl by default (Ctrl+Win chords land in a later pass).
const PTT_VK: u16 = VK_RCONTROL.0;

static APP: OnceLock<AppHandle> = OnceLock::new();
static CLOCK: OnceLock<Instant> = OnceLock::new();
static RECORDING: AtomicBool = AtomicBool::new(false);
static JAC_DICTATION_READY: AtomicBool = AtomicBool::new(false);
static JAC_TICKER_RUNNING: AtomicBool = AtomicBool::new(false);
static PENDING_DICTATION_INPUTS: OnceLock<Mutex<VecDeque<serde_json::Value>>> = OnceLock::new();
static CAPTURE: OnceLock<Mutex<Option<openflow_audio::CaptureHandle>>> = OnceLock::new();
static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
/// The cloud speech-to-text engine (`Settings::asr_mode == Cloud`), rebuilt
/// whenever the OpenAI key or the ASR base URL / model changes. `None` when
/// Cloud mode isn't selected or no OpenAI-slot key is saved — the finalize
/// path falls back to `asr_model::engine()` (Local) in that case.
static ASR_CLOUD: OnceLock<Mutex<Option<std::sync::Arc<dyn AsrEngine>>>> = OnceLock::new();
static OPENAI: OnceLock<Mutex<Option<openflow_cleanup::OpenAiProvider>>> = OnceLock::new();
static PUBLIK: OnceLock<Mutex<Option<openflow_cleanup::PublikProvider>>> = OnceLock::new();
static SETTINGS: OnceLock<Mutex<openflow_core::Settings>> = OnceLock::new();
static DICTIONARY: OnceLock<Mutex<Vec<crate::hotkey::DictEntryDto>>> = OnceLock::new();
static STATS: OnceLock<Mutex<Vec<serde_json::Value>>> = OnceLock::new();

fn support_dir() -> std::path::PathBuf {
    // %APPDATA%\OpenFlow
    let base = std::env::var("APPDATA").unwrap_or_default();
    std::path::PathBuf::from(base).join("OpenFlow")
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
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    CLOCK.get().map(|c| c.elapsed().as_millis() as u64).unwrap_or(0)
}

fn emit_bar(state: &'static str) {
    if let Some(app) = APP.get() {
        // Shared emitter: also toggles the overlay window (hidden at idle).
        crate::emit_flowbar_state(app, state);
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

fn send_jac_tick() {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":"tick", "at_ms":now_ms()}));
}

pub fn register_jac_dictation() {
    JAC_DICTATION_READY.store(true, Ordering::SeqCst);
    flush_dictation_inputs();
    if JAC_TICKER_RUNNING.swap(true, Ordering::SeqCst) { return; }
    std::thread::spawn(|| {
        while JAC_DICTATION_READY.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
            send_jac_tick();
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

/// The foreground process's executable name (e.g. "chrome.exe"), for per-app
/// cleanup formatting — the Windows analogue of the macOS bundle id.
fn foreground_app() -> Option<String> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle =
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;
        let mut buf = [0u16; 260];
        let len = GetModuleBaseNameW(handle, None, &mut buf);
        if len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

// ── Text injection: clipboard + Ctrl+V via SendInput ────────────────────────────

fn key_event(vk: u16, up: bool) -> INPUT {
    let mut ki = KEYBDINPUT {
        wVk: VIRTUAL_KEY(vk),
        ..Default::default()
    };
    if up {
        ki.dwFlags = KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 { ki },
    }
}

pub fn paste_text(text: &str) -> anyhow::Result<()> {
    use arboard::Clipboard;
    let mut cb = Clipboard::new()?;
    let saved = cb.get_text().ok();
    cb.set_text(text.to_string())?;
    std::thread::sleep(Duration::from_millis(60));
    let inputs = [
        key_event(VK_CONTROL.0, false),
        key_event(VK_V.0, false),
        key_event(VK_V.0, true),
        key_event(VK_CONTROL.0, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
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
            if settings.cleanup_mode == "publik" {
                let snapshot = PUBLIK.get().and_then(|slot| {
                    slot.lock()
                        .unwrap()
                        .as_ref()
                        .and_then(|item| item.last_balance())
                });
                if let Some(snapshot) = snapshot {
                    crate::publik::set_balance_from_snapshot(app, &snapshot);
                }
            }
            return cleaned;
        }
    }
    if cloud_asr_response { String::new() } else { raw.to_string() }
}

pub fn cleanup_with_messages(
    app: &AppHandle,
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
        "publik" => {
            let result = PUBLIK.get().and_then(|slot| {
                slot.lock()
                    .ok()?
                    .as_ref()
                    .map(|item| item.cleanup_messages_typed_with_budget(&native_messages, max_tokens))
            });
            match result {
                Some(Ok(text)) => {
                    if let Some(snapshot) = PUBLIK
                        .get()
                        .and_then(|slot| slot.lock().ok()?.as_ref()?.last_balance())
                    {
                        crate::publik::set_balance_from_snapshot(app, &snapshot);
                    }
                    Some(Ok(text))
                }
                Some(Err(error)) => {
                    crate::publik::handle_error(app, &error);
                    Some(Err(anyhow::anyhow!(error.to_string())))
                }
                None => None,
            }
        }
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
        eprintln!("[openflow:win] Jac stats bridge unavailable; dictation record was not persisted");
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
    std::thread::spawn(|| match openflow_audio::start(|_: &[f32]| {}) {
        Ok(handle) => {
            *CAPTURE.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(handle);
        }
        Err(e) => eprintln!("[openflow:win] mic capture failed: {e}"),
    });
}

fn on_ptt_up(session: u64) {
    if !RECORDING.swap(false, Ordering::SeqCst) {
        pipeline_finished(Some(session), false);
        return; // wasn't recording
    }
    let app = foreground_app();
    let handle = CAPTURE.get().and_then(|slot| slot.lock().unwrap().take());
    std::thread::spawn(move || {
        // Below ~600ms is very likely an accidental brief tap, so don't alarm
        // the user over it — only surface a loud diagnostic for holds long
        // enough to be a real, intentional dictation attempt. Bug fix: this
        // whole function used to fail silently on every one of these paths
        // (no eprintln, no UI signal, nothing) — indistinguishable from
        // "held the key, spoke, nothing happened".
        let result = (|| -> bool {
        let Some(res) = handle.and_then(|h| h.stop()) else {
            eprintln!("[openflow:win] no audio captured");
            return false;
        };
        let peak = res.samples.iter().fold(0f32, |m, &s| m.max(s.abs()));
        let rms = if res.samples.is_empty() { 0.0 } else {
            (res.samples.iter().map(|sample| sample * sample).sum::<f32>() / res.samples.len() as f32).sqrt()
        };
        let quality = APP.get().and_then(|app| crate::jac_audio_policy::assess(app, peak, rms, res.duration_secs()));
        let Some(quality) = quality else {
            eprintln!("[openflow:win] Jac audio policy is unavailable; skipping transcription");
            return false;
        };
        let was_real_attempt = quality.real_attempt;
        if !quality.accepted {
            eprintln!("[openflow:win] ⚠ audio is silent — the mic isn't being captured");
            if was_real_attempt {
                if let Some(app) = APP.get() {
                    crate::diag::report(app, "no_audio_captured");
                }
            }
            return false;
        }
        // Cloud ASR (Settings::asr_mode == Cloud, with an OpenAI-slot key saved)
        // takes priority when configured; otherwise fall back to the shared
        // local Whisper engine (asr_model.rs).
        let cloud = ASR_CLOUD.get().and_then(|m| m.lock().unwrap().clone());
        let cloud_asr_response = cloud.is_some();
        let asr: std::sync::Arc<dyn AsrEngine> = match cloud {
            Some(engine) => engine,
            None => match crate::asr_model::engine() {
                Some(engine) => engine,
                None => {
                    eprintln!("[openflow:win] ASR not ready (model still loading or missing)");
                    if was_real_attempt {
                        if let Some(app) = APP.get() {
                            crate::asr_model::report_unavailable(app);
                        }
                    }
                    return false;
                }
            },
        };
        let pcm = openflow_audio::resample_to_16k(&res.samples, res.sample_rate);
        match asr.transcribe(&pcm) {
            Ok(t) => {
                let text = clean_transcript(&t.text, cloud_asr_response);
                if !text.is_empty() {
                    record_dictation(&text, res.duration_secs(), app.clone());
                    if let Err(e) = paste_text(&text) {
                        eprintln!("[openflow:win] paste failed: {e}");
                        if let Some(app) = APP.get() {
                            crate::diag::report(app, "clipboard_unavailable");
                        }
                    } else {
                        crate::diag::clear_last_error();
                        return true;
                    }
                } else if was_real_attempt {
                    if let Some(app) = APP.get() {
                        crate::diag::report(app, "empty_transcript");
                    }
                }
            }
            Err(e) => {
                eprintln!("[openflow:win] ASR error: {e}");
                if was_real_attempt {
                    if let Some(app) = APP.get() {
                        crate::diag::report(app, "transcription_failed");
                    }
                }
            }
        }
        false
        })();
        pipeline_finished(Some(session), result);
    });
}

fn route_ptt_input(kind: &str) {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":kind, "binding":"push_to_talk", "at_ms":now_ms()}));
}

// ── Low-level keyboard hook ─────────────────────────────────────────────────────

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode as u16;
        if vk == PTT_VK {
            match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => route_ptt_input("trigger_down"),
                WM_KEYUP | WM_SYSKEYUP => route_ptt_input("trigger_up"),
                _ => {}
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Install the hook on a dedicated thread with its own message pump (required for
/// WH_KEYBOARD_LL to deliver events).
///
/// Bug fix: this used to try `SetWindowsHookExW` exactly once and, on failure,
/// give up FOREVER with only an `eprintln!` — Right Ctrl would then do nothing
/// for the rest of the run, indistinguishable from "text isn't typed" with no
/// visible explanation. Now it reports the failure loudly (once) and keeps
/// retrying, so recovering (e.g. after whatever was holding a conflicting
/// global hook closes) doesn't require relaunching OpenFlow.
fn spawn_hook_thread() {
    std::thread::spawn(|| unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let mut reported = false;
        let hook = loop {
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hinst, 0) {
                Ok(h) => break h,
                Err(_) => {
                    eprintln!("[openflow:win] failed to install keyboard hook — retrying…");
                    if !reported {
                        if let Some(app) = APP.get() {
                            crate::diag::report(app, "hotkey_tap_failed");
                        }
                        reported = true;
                    }
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        };
        if reported {
            eprintln!("[openflow:win] keyboard hook recovered — Right Ctrl is live now.");
            crate::diag::clear_last_error();
        }
        let _ = hook; // keeps the hook alive for the lifetime of this thread
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {}
    });
}

// ── Public surface (mirrors the macOS `hotkey::` functions the commands call) ────

pub fn install(app: AppHandle) {
    let _ = APP.set(app);
    let _ = CLOCK.set(Instant::now());
    let _ = SETTINGS.set(Mutex::new(openflow_core::Settings::load(&settings_path())));
    let _ = DICTIONARY.set(Mutex::new(crate::hotkey::load_dictionary_entries(&dict_path())));
    let _ = STATS.set(Mutex::new(crate::stats_store::load(&stats_path())));
    let _ = OPENAI.set(Mutex::new(None));
    let _ = LOCAL.set(Mutex::new(None));
    rebuild_providers();

    // Start the local cleanup worker.
    std::thread::spawn(|| {
        if let Some(w) = crate::local_llm::spawn_default() {
            if let Some(slot) = LOCAL.get() {
                *slot.lock().unwrap() = Some(w);
            }
        }
    });

    spawn_hook_thread();
    eprintln!("[openflow:win] keyboard hook installed (push-to-talk: Right Ctrl)");
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

pub fn stop_dictation() {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":"stop", "at_ms":now_ms()}));
}

pub fn cancel_dictation() {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":"cancel", "at_ms":now_ms()}));
}

pub fn trigger_hands_free() {
    let _ = send_jac_dictation_input(serde_json::json!({"kind":"trigger_down", "binding":"hands_free", "at_ms":now_ms()}));
}

pub fn rebuild_providers() {
    let settings = current_settings_inner();
    let base_url = settings.openai_base_url.clone();
    let key = keyring::Entry::new("com.openflow.openflow", "openai_api_key")
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|k| !k.trim().is_empty());
    if let Some(slot) = OPENAI.get() {
        *slot.lock().unwrap() = key.clone().map(|k| {
            openflow_cleanup::OpenAiProvider::with_base_url(k, Some(base_url))
        });
    }
    rebuild_cloud_asr(&settings, key);
    let publik = crate::publik::read_key().map(|k| {
        openflow_cleanup::PublikProvider::new(
            k,
            &openflow_cleanup::publik::resolve_base_url(&settings.publik_base_url),
            &settings.publik_model,
        )
    });
    match PUBLIK.get() {
        Some(m) => *m.lock().unwrap() = publik,
        None => {
            let _ = PUBLIK.set(Mutex::new(publik));
        }
    }
}

/// (Re)build the cloud speech-to-text engine to match `Settings::asr_mode`.
/// Cloud ASR reuses the OpenAI-slot key (already looked up by the caller);
/// `None` (Local, or Cloud with no key saved) means the finalize path falls
/// back to `asr_model::engine()`.
fn rebuild_cloud_asr(settings: &openflow_core::Settings, openai_key: Option<String>) {
    let engine: Option<std::sync::Arc<dyn AsrEngine>> =
        if settings.asr_mode == "cloud" {
            openai_key.map(|key| {
                let engine: std::sync::Arc<dyn AsrEngine> =
                    std::sync::Arc::new(openflow_cleanup::CloudAsr::with_base_url(
                        key,
                        settings.asr_model.clone(),
                        Some(settings.asr_base_url.clone()),
                    ));
                engine
            })
        } else {
            None
        };
    match ASR_CLOUD.get() {
        Some(m) => *m.lock().unwrap() = engine,
        None => {
            let _ = ASR_CLOUD.set(Mutex::new(engine));
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

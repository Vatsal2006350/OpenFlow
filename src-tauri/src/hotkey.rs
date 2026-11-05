//! Hold-Fn → pill wiring for the demo shell.
//!
//! This installs an in-process CoreGraphics event tap that sends push-to-talk
//! and Option+Space transitions to the Jac dictation reducer. Jac actions
//! return here for native microphone, ASR, and paste effects.
//!
//! In the shipping product this hook lives in a separate sidecar process (so heavy
//! inference can't stall it); running it in-process is an acceptable macOS-only
//! path for this demo and the early milestones.

/// Dictionary entry shape sent to the Hub UI (auto-learned entries flagged).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DictEntryDto {
    pub correct: String,
    pub mishears: Vec<String>,
    pub auto: bool,
}

/// Read the legacy dictionary file into the Jac-facing DTO. Keep file access
/// and the on-disk `source` compatibility field in this native adapter; editing
/// and merge policy live in Jac.
pub(crate) fn load_dictionary_entries(path: &std::path::Path) -> Vec<DictEntryDto> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(&contents) else {
        return Vec::new();
    };
    values.into_iter().filter_map(|value| {
        let correct = value.get("correct")?.as_str()?.to_string();
        let mishears = value.get("mishears")
            .and_then(serde_json::Value::as_array)
            .map(|items| items.iter().filter_map(serde_json::Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        let auto = value.get("auto").and_then(serde_json::Value::as_bool)
            .unwrap_or_else(|| value.get("source").and_then(serde_json::Value::as_str) == Some("auto"));
        Some(DictEntryDto { correct, mishears, auto })
    }).collect()
}

/// Persist the Jac-computed dictionary using the established local JSON shape.
pub(crate) fn save_dictionary_entries(path: &std::path::Path, entries: &[DictEntryDto]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let values: Vec<_> = entries.iter().map(|entry| serde_json::json!({
        "correct": entry.correct,
        "mishears": entry.mishears,
        "source": if entry.auto { "auto" } else { "manual" }
    })).collect();
    std::fs::write(path, serde_json::to_vec_pretty(&values)?)
}

#[cfg(test)]
mod dictionary_storage_tests {
    use super::{load_dictionary_entries, save_dictionary_entries, DictEntryDto};

    fn temp_path() -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("openflow-dictionary-{}-{nonce}.json", std::process::id()))
    }

    #[test]
    fn loads_the_existing_source_field_as_the_jac_auto_flag() {
        let path = temp_path();
        std::fs::write(&path, r#"[
            {"correct":"OpenFlow","mishears":["wimper flow"],"source":"auto"},
            {"correct":"Jac","mishears":[],"source":"manual"}
        ]"#).unwrap();

        let entries = load_dictionary_entries(&path);
        let _ = std::fs::remove_file(path);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].auto);
        assert!(!entries[1].auto);
        assert_eq!(entries[0].mishears, ["wimper flow"]);
    }

    #[test]
    fn saves_the_existing_source_field_for_legacy_compatibility() {
        let path = temp_path();
        let entries = vec![DictEntryDto {
            correct: "OpenFlow".into(),
            mishears: vec!["wimper flow".into()],
            auto: true,
        }];

        save_dictionary_entries(&path, &entries).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(saved[0]["source"], "auto");
        assert!(saved[0].get("auto").is_none());
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::DictEntryDto;
    use std::os::raw::c_void;
    use std::path::PathBuf;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, AtomicI64, AtomicPtr, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use tauri::{AppHandle, Emitter, Manager};
    use openflow_core::{AsrEngine, CleanupContext};

    const OVERLAY_LABEL: &str = "openflow_bar";

    // --- CoreGraphics / CoreFoundation FFI (listen-only modifier tap) -----
    type CFMachPortRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CGEventRef = *mut c_void;
    type CGEventTapProxy = *mut c_void;
    type CGEventTapCallBack =
        extern "C" fn(CGEventTapProxy, u32, CGEventRef, *mut c_void) -> CGEventRef;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(
            tap: u32,
            place: u32,
            options: u32,
            events_of_interest: u64,
            callback: CGEventTapCallBack,
            user_info: *mut c_void,
        ) -> CFMachPortRef;
        fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
        fn CGEventGetFlags(event: CGEventRef) -> u64;
        fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(
            allocator: CFAllocatorRef,
            port: CFMachPortRef,
            order: isize,
        ) -> CFRunLoopSourceRef;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
        static kCFRunLoopDefaultMode: CFStringRef;
    }

    const K_CG_SESSION_EVENT_TAP: u32 = 1;
    const K_CG_HEAD_INSERT: u32 = 0;
    const K_CG_TAP_OPTION_DEFAULT: u32 = 0;
    const K_CG_KEY_DOWN: u32 = 10;
    const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;
    const K_CG_MOUSE_MOVED: u32 = 5;
    const K_CG_LEFT_MOUSE_DOWN: u32 = 1;
    const K_CG_LEFT_MOUSE_DRAGGED: u32 = 6;
    const EVENTS_OF_INTEREST: u64 = (1 << K_CG_EVENT_FLAGS_CHANGED)
        | (1 << K_CG_KEY_DOWN)
        | (1 << K_CG_MOUSE_MOVED)
        | (1 << K_CG_LEFT_MOUSE_DOWN)
        | (1 << K_CG_LEFT_MOUSE_DRAGGED);
    const FLAG_SECONDARY_FN: u64 = 0x0080_0000;
    const FLAG_ALTERNATE: u64 = 0x0008_0000;
    const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
    const K_CG_KEYBOARD_EVENT_AUTOREPEAT: u32 = 8;
    const KEYCODE_FN: i64 = 63;
    const KEYCODE_SPACE: i64 = 49;
    const KEYCODE_OPTION_LEFT: i64 = 58;
    const KEYCODE_OPTION_RIGHT: i64 = 61;
    const K_CG_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
    const K_CG_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

    static APP: OnceLock<AppHandle> = OnceLock::new();
    static JAC_DICTATION_READY: AtomicBool = AtomicBool::new(false);
    static PENDING_DICTATION_INPUTS: OnceLock<Mutex<VecDeque<serde_json::Value>>> = OnceLock::new();
    static CLOCK: OnceLock<Instant> = OnceLock::new();
    static PTT_IS_DOWN: AtomicBool = AtomicBool::new(false);
    static PTT_ACTIVE_KEYCODE: AtomicI64 = AtomicI64::new(-1);
    static PTT_KEYCODE: AtomicI64 = AtomicI64::new(KEYCODE_OPTION_LEFT);
    // The native event tap and Tauri's registered shortcut can both observe
    // Option+Space. Treat callbacks for the same physical press as one toggle.
    static LAST_HANDS_FREE_MS: AtomicI64 = AtomicI64::new(-1_000);
    static OVERLAY_X_MILLI: AtomicI64 = AtomicI64::new(0);
    static OVERLAY_Y_MILLI: AtomicI64 = AtomicI64::new(0);
    static OVERLAY_W_MILLI: AtomicI64 = AtomicI64::new(0);
    static OVERLAY_H_MILLI: AtomicI64 = AtomicI64::new(0);
    static OVERLAY_POINTER_INSIDE: AtomicBool = AtomicBool::new(false);
    static LAST_OVERLAY_POINTER_EVENT_MS: AtomicI64 = AtomicI64::new(-1_000);
    static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
    /// Bundle id of the app that was frontmost at record-start = the paste target.
    /// Cleanup uses it to format for the medium (email vs. text vs. chat).
    static TARGET_APP: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    static CAPTURE: OnceLock<Mutex<Option<openflow_audio::CaptureHandle>>> = OnceLock::new();
    static OPENAI: OnceLock<Mutex<Option<openflow_cleanup::OpenAiProvider>>> = OnceLock::new();
    static ANTHROPIC: OnceLock<Mutex<Option<openflow_cleanup::AnthropicProvider>>> = OnceLock::new();
    static PUBLIK: OnceLock<Mutex<Option<openflow_cleanup::PublikProvider>>> = OnceLock::new();
    static LOCAL: OnceLock<Mutex<Option<crate::local_llm::LocalWorker>>> = OnceLock::new();
    /// The cloud speech-to-text engine (`Settings::asr_mode == Cloud`), rebuilt
    /// whenever the OpenAI key or the ASR base URL / model changes. `None` when
    /// Cloud mode isn't selected or no OpenAI-slot key is saved — the finalize
    /// path falls back to `asr_model::engine()` (Local) in that case.
    static ASR_CLOUD: OnceLock<Mutex<Option<std::sync::Arc<dyn AsrEngine>>>> = OnceLock::new();
    static SETTINGS: OnceLock<Mutex<openflow_core::Settings>> = OnceLock::new();
    static DICTIONARY: OnceLock<Mutex<Vec<DictEntryDto>>> = OnceLock::new();
    static STATS: OnceLock<Mutex<Vec<serde_json::Value>>> = OnceLock::new();

    #[derive(Clone, Serialize)]
    struct WavePayload {
        bars: Vec<f32>,
    }

    #[derive(Clone, Serialize)]
    struct TranscriptPayload {
        text: String,
    }

    #[derive(Clone, Serialize)]
    struct OverlayPointerPayload {
        inside: bool,
        x: f64,
        y: f64,
    }

    fn support_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/OpenFlow")
    }
    fn settings_path() -> PathBuf {
        support_dir().join("settings.json")
    }
    fn dict_path() -> PathBuf {
        support_dir().join("dictionary.json")
    }
    fn stats_path() -> PathBuf {
        support_dir().join("stats.json")
    }

    /// Seconds since the Unix epoch (UTC), or 0 if the clock is before the epoch.
    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Play the macOS recording-start cue when requested by a Jac action.
    /// `afplay` rather than `NSSound`: no extra dependency, no
    /// main-thread requirement, and it can be fired from the event-tap thread
    /// without any risk of blocking key handling. Spawned so a slow process
    /// launch never delays the rest of the tap callback.
    fn play_start_cue() {
        std::thread::spawn(|| {
            let _ = std::process::Command::new("/usr/bin/afplay")
                .arg("/System/Library/Sounds/Pop.aiff")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        });
    }

    /// Log one completed dictation to the stats store (words, speaking time, text,
    /// target app) and persist it. Powers both the Hub stats and the history list.
    pub fn record_dictation(text: &str, duration_secs: f32) {
        let target_app = TARGET_APP.get().and_then(|m| m.lock().unwrap().clone());
        let record = APP.get().and_then(|app| {
            crate::jac_stats::request(app, text, duration_secs, unix_now(), target_app.clone())
        });
        let Some(record) = record else {
            eprintln!("[openflow] Jac stats bridge unavailable; dictation record was not persisted");
            return;
        };
        if let Some(m) = STATS.get() {
            let mut store = m.lock().unwrap();
            store.push(record);
            let _ = crate::stats_store::save(&stats_path(), &store);
        }
    }

    /// The most recent dictations for the Hub Home history list.
    /// The dictionary entries for the Hub Dictionary screen (auto-learned flagged).
    pub fn dictionary_entries() -> Vec<DictEntryDto> {
        DICTIONARY
            .get()
            .map(|m| m.lock().unwrap().clone())
            .unwrap_or_default()
    }

    /// Persist the dictionary state computed by the Jac policy layer.
    pub fn dictionary_replace(entries: Vec<DictEntryDto>) {
        if let Some(m) = DICTIONARY.get() {
            let mut stored = m.lock().unwrap();
            *stored = entries;
            let _ = super::save_dictionary_entries(&dict_path(), &stored);
        }
    }

    /// Read an API key from an env var or the OS keychain (never a plaintext file).
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
    fn read_openai_key() -> Option<String> {
        read_key("openai_api_key", "OPENAI_API_KEY")
    }
    fn read_anthropic_key() -> Option<String> {
        read_key("anthropic_api_key", "ANTHROPIC_API_KEY")
    }

    /// A snapshot of the current settings.
    pub fn current_settings() -> openflow_core::Settings {
        SETTINGS
            .get()
            .map(|m| m.lock().unwrap().clone())
            .unwrap_or_default()
    }
    /// Apply new settings and rebuild the cloud providers (picks up model changes).
    pub fn update_settings(new: openflow_core::Settings) {
        PTT_KEYCODE.store(
            if new.push_to_talk_key == "AltRight" {
                KEYCODE_OPTION_RIGHT
            } else {
                KEYCODE_OPTION_LEFT
            },
            Ordering::SeqCst,
        );
        if let Some(m) = SETTINGS.get() {
            *m.lock().unwrap() = new.clone();
        }
        let _ = new.save(&settings_path());
        rebuild_providers();
    }

    /// (Re)build the cloud cleanup providers from the current keys + settings. Called
    /// at startup and whenever a key or model changes, so edits take effect live.
    pub fn rebuild_providers() {
        let settings = current_settings();
        let openai = read_openai_key().map(|k| {
            openflow_cleanup::OpenAiProvider::with_base_url(
                k,
                Some(settings.openai_base_url.clone()),
            )
        });
        let anthropic = read_anthropic_key()
            .map(openflow_cleanup::AnthropicProvider::new);
        // publik: its own keychain account + the base URL / alias the
        // provisioning response handed back (never the user's OpenAI slot).
        let publik = crate::publik::read_key().map(|k| {
            openflow_cleanup::PublikProvider::new(
                k,
                &openflow_cleanup::publik::resolve_base_url(&settings.publik_base_url),
                &settings.publik_model,
            )
        });
        eprintln!(
            "[openflow] cleanup providers: openai={}, anthropic={}, publik={}",
            openai.is_some(),
            anthropic.is_some(),
            publik.is_some()
        );
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
        match PUBLIK.get() {
            Some(m) => *m.lock().unwrap() = publik,
            None => {
                let _ = PUBLIK.set(Mutex::new(publik));
            }
        }
        rebuild_cloud_asr(&settings);
    }

    /// (Re)build the cloud speech-to-text engine to match `Settings::asr_mode`.
    /// Cloud ASR reuses the OpenAI-slot key; `None` (Local, or Cloud with no key
    /// saved) means the finalize path falls back to `asr_model::engine()`.
    fn rebuild_cloud_asr(settings: &openflow_core::Settings) {
        let engine: Option<std::sync::Arc<dyn AsrEngine>> =
            if settings.asr_mode == "cloud" {
                read_openai_key().map(|key| {
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
        STATS.get().map(|m| m.lock().unwrap().clone()).unwrap_or_default()
    }

    /// Delegate cleanup policy to Jac and keep the transcription safe if its
    /// bridge is unavailable. Native code supplies vocabulary/app facts and
    /// executes the selected provider adapter.
    fn clean_transcript(app: &AppHandle, raw: &str, cloud_asr_response: bool) -> String {
        let settings = current_settings();
        let dictionary = DICTIONARY
            .get()
            .map(|d| d.lock().unwrap().clone())
            .unwrap_or_default();
        let app_bundle_id = TARGET_APP.get().and_then(|m| m.lock().unwrap().clone());
        if let Some(app) = app_bundle_id.as_deref() {
            eprintln!("[openflow] cleanup target app: {app}");
        }
        let ctx = CleanupContext { app_bundle_id, ..Default::default() };
        if crate::jac_cleanup::is_ready() {
            if let Some(cleaned) = crate::jac_cleanup::request(app, raw, raw, cloud_asr_response, &settings, &ctx, &dictionary) {
                if settings.cleanup_mode == "publik" {
                    let snapshot = PUBLIK
                        .get()
                        .and_then(|m| m.lock().unwrap().as_ref().and_then(|p| p.last_balance()));
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
        let selected = match provider {
            "open_ai" => request_body.and_then(|body| OPENAI
                .get()
                .and_then(|m| m.lock().ok()?.as_ref().map(|p| p.cleanup_with_request_body(body)))),
            "anthropic" => request_body.and_then(|body| ANTHROPIC
                .get()
                .and_then(|m| m.lock().ok()?.as_ref().map(|p| p.cleanup_with_request_body(body)))),
            "publik" => {
                let result = PUBLIK.get().and_then(|m| m.lock().ok()?.as_ref().map(|p| p.cleanup_messages_typed_with_budget(&native_messages, max_tokens)));
                match result {
                    Some(Ok(text)) => {
                        if let Some(snapshot) = PUBLIK.get().and_then(|m| m.lock().ok()?.as_ref()?.last_balance()) {
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
            "local" => LOCAL.get().and_then(|worker| worker.lock().ok().and_then(|mut slot| {
                slot.as_mut().map(|worker| worker.cleanup_with_budget(&native_messages, max_tokens))
            })),
            "raw" => return Ok(raw.to_string()),
            _ => return Err("Unknown cleanup provider".into()),
        };
        selected.ok_or_else(|| format!("Cleanup provider {provider} is not configured"))?
            .map_err(|error| error.to_string())
    }

    fn now_ms() -> u64 {
        CLOCK
            .get()
            .map(|c| c.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    fn emit_bar(app: &AppHandle, state: &'static str) {
        eprintln!("[openflow] pill -> {state}");
        // Shared emitter: also toggles the overlay window (hidden at idle).
        crate::emit_flowbar_state(app, state);
    }

    /// Send dictation inputs to Jac. The native event tap only owns OS input;
    /// transition policy lives in the Hub's Jac reducer.
    fn dispatch_dictation_input(payload: serde_json::Value) {
        let Some(app) = APP.get() else { return; };
        let mut pending = PENDING_DICTATION_INPUTS.get_or_init(|| Mutex::new(VecDeque::new())).lock().unwrap();
        let is_tick = payload.get("kind").and_then(|v| v.as_str()) == Some("tick");
        if !JAC_DICTATION_READY.load(Ordering::Acquire) {
            if !is_tick && pending.len() < 128 { pending.push_back(payload); }
            return;
        }
        if pending.len() >= 128 {
            if is_tick { return; }
            pending.pop_front();
        }
        pending.push_back(payload);
        while let Some(next) = pending.pop_front() {
            if app.emit_to("main", "openflow://dictation/input", next.clone()).is_err() {
                pending.push_front(next);
                break;
            }
        }
    }

    fn flush_dictation_inputs() {
        dispatch_dictation_input(serde_json::json!({"kind":"tick", "at_ms":now_ms()}));
    }

    pub fn register_jac_dictation() {
        JAC_DICTATION_READY.store(true, Ordering::SeqCst);
        flush_dictation_inputs();
    }

    pub fn unregister_jac_dictation() {
        JAC_DICTATION_READY.store(false, Ordering::SeqCst);
    }

    pub fn apply_jac_dictation_actions(actions: Vec<serde_json::Value>) {
        let Some(app) = APP.get() else { return; };
        for value in actions {
            apply_action(app, &value);
        }
    }

    fn apply_action(app: &AppHandle, action: &serde_json::Value) {
        let kind = action.get("kind").and_then(serde_json::Value::as_str).unwrap_or_default();
        let session = action.get("session").and_then(serde_json::Value::as_u64).unwrap_or_default();
        match kind {
            "show_bar" => {
                let bar = match action.get("state").and_then(serde_json::Value::as_str).unwrap_or("idle") {
                    "recording" => "recording",
                    "locked" => "locked",
                    "transcribing" => "transcribing",
                    "done" => "done",
                    "cancelled" => "cancelled",
                    "error" => "error",
                    _ => "idle",
                };
                emit_bar(app, bar);
                // Let the "done" tick linger briefly before returning to idle.
                if bar == "done" {
                    let app2 = app.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(500));
                        emit_bar(&app2, "idle");
                    });
                }
            }
            // Start the microphone; stream real RMS bars to the pill waveform.
            // Runs off the tap thread so the mic-permission prompt can't stall keys.
            "start_capture" => {
                let app_thread = app.clone();
                std::thread::spawn(move || {
                    let app_cb = app_thread.clone();
                    match openflow_audio::start(move |bars| {
                        let _ = app_cb.emit_to(
                            OVERLAY_LABEL,
                            "openflow://audio/waveform",
                            WavePayload {
                                bars: bars.to_vec(),
                            },
                        );
                    }) {
                        Ok(handle) => {
                            *CAPTURE.get_or_init(|| Mutex::new(None)).lock().unwrap() =
                                Some(handle);
                        }
                        Err(e) => eprintln!("[openflow] mic capture failed to start: {e}"),
                    }
                });
            }
            // Stop the mic, transcribe the buffered audio, and advance the machine.
            "stop_capture_and_finalize" => {
                let app2 = app.clone();
                let handle = CAPTURE.get().and_then(|slot| slot.lock().unwrap().take());
                std::thread::spawn(move || {
                    // Whatever happens, return the pill to idle (done -> idle).
                    let finish = || dispatch_dictation_input(serde_json::json!({"kind":"pipeline_committed", "session": session}));
                    let Some(res) = handle.and_then(|h| h.stop()) else {
                        eprintln!("[openflow] no audio captured");
                        finish();
                        return;
                    };
                    let peak = res.samples.iter().fold(0f32, |m, &s| m.max(s.abs()));
                    let rms = if res.samples.is_empty() {
                        0.0
                    } else {
                        (res.samples.iter().map(|s| s * s).sum::<f32>() / res.samples.len() as f32)
                            .sqrt()
                    };
                    eprintln!(
                        "[openflow] captured {} samples @ {} Hz (~{:.2}s), peak {:.4}, rms {:.4}",
                        res.samples.len(),
                        res.sample_rate,
                        res.duration_secs(),
                        peak,
                        rms
                    );
                    // Jac owns the shared speech-floor and intentional-attempt policy;
                    // native capture supplies only measured audio facts.
                    let quality = crate::jac_audio_policy::assess(&app2, peak, rms, res.duration_secs());
                    let Some(quality) = quality else {
                        eprintln!("[openflow] Jac audio policy is unavailable; skipping transcription");
                        finish();
                        return;
                    };
                    let was_real_attempt = quality.real_attempt;
                    if !quality.accepted {
                        eprintln!(
                            "[openflow] audio level is below the speech floor; skipping transcription"
                        );
                        finish();
                        return;
                    }
                    // Cloud ASR (Settings::asr_mode == Cloud, with an OpenAI-slot key
                    // saved) takes priority when configured; otherwise fall back to
                    // the shared local Whisper engine (asr_model.rs).
                    let cloud = ASR_CLOUD.get().and_then(|m| m.lock().unwrap().clone());
                    let cloud_asr_response = cloud.is_some();
                    let asr: std::sync::Arc<dyn AsrEngine> = match cloud {
                        Some(engine) => engine,
                        None => match crate::asr_model::engine() {
                            Some(engine) => engine,
                            None => {
                                eprintln!(
                                    "[openflow] ASR not ready (model still loading or missing)"
                                );
                                if was_real_attempt {
                                    crate::asr_model::report_unavailable(&app2);
                                }
                                finish();
                                return;
                            }
                        },
                    };
                    let pcm = openflow_audio::resample_to_16k(&res.samples, res.sample_rate);
                    match asr.transcribe(&pcm) {
                        Ok(t) => {
                            let raw = t.text;
                            if !cloud_asr_response {
                                eprintln!("[openflow] TRANSCRIPT: \"{}\"", raw);
                            }
                            // Clean the transcript (cloud LLM if configured), then paste.
                            let text = clean_transcript(&app2, &raw, cloud_asr_response);
                            if text != raw {
                                eprintln!("[openflow] CLEANED:   \"{}\"", text);
                            }
                            if !text.is_empty() {
                                if let Err(e) = crate::paste::paste_text(&text) {
                                    eprintln!("[openflow] paste failed: {e}");
                                    // Distinguish the two real causes: Accessibility was
                                    // never (or no longer) granted, vs. everything else
                                    // (clipboard contention, etc.) — `is_trusted()` is the
                                    // authoritative check, cheaper and more precise than
                                    // matching on the error string.
                                    let failure = if !crate::paste::is_trusted() {
                                        "accessibility_not_granted"
                                    } else {
                                        "clipboard_unavailable"
                                    };
                                    crate::diag::report(&app2, failure);
                                } else {
                                    crate::diag::clear_last_error();
                                }
                                // Log words + speaking time for the Hub stats (WPM, streak…).
                                record_dictation(&text, res.duration_secs());
                                // Watch the field for a post-paste correction to learn (✨).
                                crate::autolearn::watch_correction(app2.clone(), &text);
                            } else if was_real_attempt {
                                crate::diag::report(
                                    &app2,
                                    "empty_transcript",
                                );
                            }
                            let _ = app2.emit_to(
                                OVERLAY_LABEL,
                                "openflow://transcript",
                                TranscriptPayload { text },
                            );
                        }
                        Err(e) => {
                            eprintln!("[openflow] ASR error: {e}");
                            if was_real_attempt {
                                crate::diag::report(
                                    &app2,
                                    "transcription_failed",
                                );
                            }
                        }
                    }
                    finish();
                });
            }
            "discard_capture" => {
                if let Some(slot) = CAPTURE.get() {
                    if let Some(handle) = slot.lock().unwrap().take() {
                        let _ = handle.stop();
                    }
                }
            }
            // The ASR path (StopCaptureAndFinalize) now drives pipeline completion.
            "run_pipeline" | "warn_session_cap" => {}
            "play_ping" => play_start_cue(),
            // WarnSessionCap: no-op for now.
            _ => {}
        }
    }

    extern "C" fn tap_callback(
        _proxy: CGEventTapProxy,
        etype: u32,
        event: CGEventRef,
        _info: *mut c_void,
    ) -> CGEventRef {
        if etype == K_CG_TAP_DISABLED_BY_TIMEOUT || etype == K_CG_TAP_DISABLED_BY_USER_INPUT {
            let port = TAP_PORT.load(Ordering::SeqCst);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port, true) };
            }
            return event;
        }
        // The floating WebView can miss hover while another app is frontmost.
        // Track the global pointer in the existing event tap and send local
        // coordinates to the overlay, keeping expansion independent of app
        // activation and WebKit's mouse-enter delivery.
        if etype == K_CG_MOUSE_MOVED
            || etype == K_CG_LEFT_MOUSE_DOWN
            || etype == K_CG_LEFT_MOUSE_DRAGGED
        {
            let point = unsafe { CGEventGetLocation(event) };
            let bx = OVERLAY_X_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
            let by = OVERLAY_Y_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
            let bw = OVERLAY_W_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
            let bh = OVERLAY_H_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
            if bw > 0.0 && bh > 0.0 {
                let was_inside = OVERLAY_POINTER_INSIDE.load(Ordering::Relaxed);
                let in_window =
                    point.x >= bx && point.x <= bx + bw && point.y >= by && point.y <= by + bh;
                let center_x = bx + bw / 2.0;
                let over_line_target = point.x >= center_x - 48.0
                    && point.x <= center_x + 48.0
                    && point.y >= by + bh - 48.0
                    && point.y <= by + bh;
                let inside = if was_inside {
                    in_window
                } else {
                    over_line_target
                };
                let changed = inside != was_inside;
                if changed {
                    OVERLAY_POINTER_INSIDE.store(inside, Ordering::Relaxed);
                    if let Some(app) = APP.get() {
                        if let Some(overlay) = app.get_webview_window(OVERLAY_LABEL) {
                            // Only intercept native mouse input while the
                            // cursor is over the resting line or expanded tray.
                            let _ = overlay.set_ignore_cursor_events(!inside);
                        }
                    }
                }
                let now = now_ms() as i64;
                let previous = LAST_OVERLAY_POINTER_EVENT_MS.load(Ordering::Relaxed);
                if changed || (inside && now.saturating_sub(previous) >= 32) {
                    LAST_OVERLAY_POINTER_EVENT_MS.store(now, Ordering::Relaxed);
                    if let Some(app) = APP.get() {
                        let _ = app.emit_to(
                            OVERLAY_LABEL,
                            "openflow://overlay/pointer",
                            OverlayPointerPayload {
                                inside,
                                x: point.x - bx,
                                y: point.y - by,
                            },
                        );
                    }
                }
            }
            return event;
        }
        // Option+Space is OpenFlow's default hands-free shortcut. Handle it
        // in the same event tap as Option push-to-talk so the Jac reducer sees
        // the chord before either key is released. Consume only that chord:
        // otherwise Space would type a non-breaking space into the focused app.
        if etype == K_CG_KEY_DOWN {
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
            let repeats =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_AUTOREPEAT) } != 0;
            let option_held = unsafe { CGEventGetFlags(event) } & FLAG_ALTERNATE != 0;
            let configured = current_settings()
                .hands_free_hotkey
                .replace(' ', "")
                .eq_ignore_ascii_case("Alt+Space");
            if keycode == KEYCODE_SPACE && option_held && configured {
                if !repeats {
                    trigger_hands_free();
                }
                return null_mut();
            }
            return event;
        }
        if etype == K_CG_EVENT_FLAGS_CHANGED {
            let keycode =
                unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) };
            let configured_ptt_key = PTT_KEYCODE.load(Ordering::SeqCst);
            if keycode == KEYCODE_FN || keycode == configured_ptt_key {
                let flags = unsafe { CGEventGetFlags(event) };
                let down = if keycode == KEYCODE_FN {
                    (flags & FLAG_SECONDARY_FN) != 0
                } else {
                    (flags & FLAG_ALTERNATE) != 0
                };
                let at_ms = now_ms();
                if down && !PTT_IS_DOWN.swap(true, Ordering::SeqCst) {
                    PTT_ACTIVE_KEYCODE.store(keycode, Ordering::SeqCst);
                    eprintln!("[openflow] push-to-talk DOWN ({keycode})");
                    // Snapshot the paste target now, while the user's app is focused.
                    let target = crate::appctx::frontmost_bundle_id();
                    *TARGET_APP.get_or_init(|| Mutex::new(None)).lock().unwrap() = target;
                    dispatch_dictation_input(serde_json::json!({"kind":"trigger_down", "binding":"push_to_talk", "at_ms":at_ms}));
                } else if !down
                    && PTT_ACTIVE_KEYCODE.load(Ordering::SeqCst) == keycode
                    && PTT_IS_DOWN.swap(false, Ordering::SeqCst)
                {
                    PTT_ACTIVE_KEYCODE.store(-1, Ordering::SeqCst);
                    eprintln!("[openflow] push-to-talk UP ({keycode})");
                    dispatch_dictation_input(serde_json::json!({"kind":"trigger_up", "binding":"push_to_talk", "at_ms":at_ms}));
                }
            }
        }
        event
    }

    /// Stop and finalize the current recording — the pill's red Stop button.
    /// Send the pill's Stop input through Jac so it works in either mode.
    pub fn stop_dictation() {
        dispatch_dictation_input(serde_json::json!({"kind":"stop", "at_ms":now_ms()}));
    }

    /// Discard the current recording — the pill's ✕ button. Same path Esc would
    /// take, if Esc were wired. Reported dead in Publik Test 2 ("the X button
    /// doesn't work").
    pub fn cancel_dictation() {
        dispatch_dictation_input(serde_json::json!({"kind":"cancel", "at_ms":now_ms()}));
    }

    /// Toggle HANDS-FREE (locked) dictation — the customizable global hotkey
    /// (default Option+Space) fires this. The Jac reducer treats a
    /// `HandsFree` press as a toggle: from idle it starts a locked session that
    /// keeps recording with no key held, and while locked it finalizes. This is
    /// the "speak without having to hold down fn" ask from Publik Test 2.
    pub fn trigger_hands_free() {
        let now = now_ms() as i64;
        let previous = LAST_HANDS_FREE_MS.swap(now, Ordering::SeqCst);
        if now.saturating_sub(previous) < 80 {
            return;
        }
        eprintln!("[openflow] hands-free shortcut pressed");
        dispatch_dictation_input(serde_json::json!({"kind":"trigger_down", "binding":"hands_free", "at_ms":now as u64}));
    }

    pub fn set_overlay_bounds(x: f64, y: f64, width: f64, height: f64) {
        OVERLAY_X_MILLI.store((x * 1000.0) as i64, Ordering::Relaxed);
        OVERLAY_Y_MILLI.store((y * 1000.0) as i64, Ordering::Relaxed);
        OVERLAY_W_MILLI.store((width * 1000.0) as i64, Ordering::Relaxed);
        OVERLAY_H_MILLI.store((height * 1000.0) as i64, Ordering::Relaxed);
        OVERLAY_POINTER_INSIDE.store(false, Ordering::Relaxed);
    }

    pub fn install(app: AppHandle) {
        let _ = APP.set(app);
        let _ = CLOCK.set(Instant::now());

        // Load settings + dictionary, and build cloud providers from stored keys.
        let settings = openflow_core::Settings::load(&settings_path());
        let ptt_keycode = if settings.push_to_talk_key == "AltRight" {
            KEYCODE_OPTION_RIGHT
        } else {
            KEYCODE_OPTION_LEFT
        };
        PTT_KEYCODE.store(ptt_keycode, Ordering::SeqCst);
        let dict = super::load_dictionary_entries(&dict_path());
        eprintln!(
            "[openflow] cleanup mode: {:?}, level: {:?}",
            settings.cleanup_mode, settings.cleanup_level
        );
        let _ = SETTINGS.set(Mutex::new(settings));
        let _ = DICTIONARY.set(Mutex::new(dict));
        let _ = STATS.set(Mutex::new(crate::stats_store::load(&stats_path())));
        rebuild_providers();

        // Start the local cleanup worker in the background (model load takes a few
        // seconds; the first local cleanup waits for it, subsequent ones are fast).
        std::thread::spawn(|| {
            let worker = crate::local_llm::spawn_default();
            let _ = LOCAL.set(Mutex::new(worker));
        });

        // Accessibility is the ONE permission that makes the Fn CGEventTap global AND
        // lets us post the Cmd+V paste into other apps. Without it, a keyboard tap is
        // silently limited to frontmost-only — the exact bug. Prompt for it up front.
        if crate::paste::is_trusted() {
            eprintln!("[openflow] Accessibility granted — Fn works in every app, paste enabled");
        } else {
            eprintln!(
                "[openflow] ⚠ Accessibility NOT granted — clearing any stale TCC entry, \
                 re-prompting, and opening System Settings → Privacy & Security → \
                 Accessibility (no relaunch needed)."
            );
            // Self-heal: `is_trusted()` can stay permanently false even after the
            // user grants (and re-grants) Accessibility in System Settings, because
            // `tauri build` ad-hoc-signs every build with a fresh code signature and
            // a TCC row tied to an OLDER signature never transfers to a new one —
            // toggling the checkbox then just re-toggles that STALE row and nothing
            // downstream of `is_trusted()` ever notices, matching the reports
            // ("reinstalling/re-toggling doesn't fix it"). Clear the stale row for
            // THIS build's own identifier (read from its own config, not assumed,
            // so this is correct under whatever bundle id this binary actually
            // ships under) so the grant the user makes next lands against this
            // build's real signature, then re-fire the native prompt. Spawned so a
            // slow tccutil call never delays the rest of startup below.
            let bundle_id = APP
                .get()
                .map(|a| a.config().identifier.clone())
                .unwrap_or_else(|| "com.openflow.openflow".to_string());
            std::thread::spawn(move || {
                match std::process::Command::new("/usr/bin/tccutil")
                    .args(["reset", "Accessibility", &bundle_id])
                    .output()
                {
                    Ok(out) if out.status.success() => eprintln!(
                        "[openflow] tccutil reset done: {}",
                        String::from_utf8_lossy(&out.stdout).trim()
                    ),
                    Ok(out) => eprintln!(
                        "[openflow] accessibility self-heal: tccutil exited with {}: {}",
                        out.status,
                        String::from_utf8_lossy(&out.stderr).trim()
                    ),
                    Err(e) => {
                        eprintln!("[openflow] accessibility self-heal: failed to run tccutil: {e}")
                    }
                }
                crate::paste::prompt_accessibility();
                crate::open_url(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                );
            });
        }
        // Input Monitoring is NOT the gate for a CGEventTap — kept only as diagnostics.
        eprintln!(
            "[openflow] (info) Input Monitoring: {}",
            crate::paste::input_monitoring_granted()
        );

        // Periodic tick drives the double-tap timeout / session cap.
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_millis(100));
            dispatch_dictation_input(serde_json::json!({"kind":"tick", "at_ms":now_ms()}));
        });

        // The event tap runs on a thread with its own CFRunLoop. CRITICAL: create it
        // ONLY after the process is trusted for Accessibility. macOS fixes a keyboard
        // tap's privilege at CGEventTapCreate time — a tap born untrusted is
        // permanently frontmost-only and is NOT upgraded when the grant later arrives.
        // Polling here also means the Fn key starts working the moment the user grants
        // Accessibility, without a relaunch.
        std::thread::spawn(|| {
            while !crate::paste::is_trusted() {
                std::thread::sleep(Duration::from_millis(500));
            }
            eprintln!("[openflow] Accessibility present — creating global push-to-talk tap");
            // Bug fix: this used to try CGEventTapCreate exactly once and, if it
            // came back null despite Accessibility being granted (the real-world
            // stale-TCC-entry case this app's own comments already knew about),
            // give up FOREVER with only an eprintln — the Fn key would then do
            // nothing for the rest of the run, indistinguishable from "text isn't
            // typed" to the user, with zero visible explanation. Now: report it
            // loudly once, and keep retrying — toggling the Accessibility entry
            // off/on in System Settings, or removing and re-adding it, fixes the
            // stale grant without requiring a relaunch, and this way that fix is
            // picked up automatically.
            let mut reported = false;
            let port = loop {
                let port = unsafe {
                    CGEventTapCreate(
                        K_CG_SESSION_EVENT_TAP,
                        K_CG_HEAD_INSERT,
                        K_CG_TAP_OPTION_DEFAULT,
                        EVENTS_OF_INTEREST,
                        tap_callback,
                        null_mut(),
                    )
                };
                if !port.is_null() {
                    break port;
                }
                eprintln!(
                    "[openflow] push-to-talk tap null despite Accessibility — likely a stale TCC entry from \
                     an earlier build. Run: tccutil reset Accessibility com.openflow.openflow, \
                     or toggle OpenFlow off/on under System Settings → Privacy & Security → \
                     Accessibility. Retrying…"
                );
                if !reported {
                    if let Some(app) = APP.get() {
                        crate::diag::report(app, "hotkey_tap_failed");
                    }
                    reported = true;
                }
                std::thread::sleep(Duration::from_secs(5));
            };
            if reported {
                eprintln!("[openflow] Fn tap recovered — the key is live now.");
                crate::diag::clear_last_error();
            }
            TAP_PORT.store(port, Ordering::SeqCst);
            unsafe {
                let source = CFMachPortCreateRunLoopSource(null(), port, 0);
                CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode);
                CGEventTapEnable(port, true);
                CFRunLoopRun();
            }
        });
    }
}

#[cfg(target_os = "macos")]
pub use imp::{
    cancel_dictation, cleanup_with_messages, current_settings, dictionary_entries, dictionary_replace,
    install, rebuild_providers, set_overlay_bounds, stats_records,
    apply_jac_dictation_actions, register_jac_dictation,
    stop_dictation, trigger_hands_free, unregister_jac_dictation, update_settings,
};

// Windows uses the real (but unverified) platform layer in `crate::win`.
#[cfg(target_os = "windows")]
pub use crate::win::{
    cancel_dictation, current_settings, dictionary_entries, dictionary_replace,
    install, rebuild_providers, stats_records, stop_dictation,
    trigger_hands_free, update_settings,
};

// Linux: real platform layer (rdev hotkey + xdotool/ydotool paste + full pipeline).
#[cfg(target_os = "linux")]
pub use crate::linux::{
    cancel_dictation, current_settings, dictionary_entries, dictionary_replace,
    install, rebuild_providers, stats_records, stop_dictation,
    trigger_hands_free, update_settings,
};

// Other platforms (none currently shipped): inert stubs so the crate still builds.
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod other {
    pub fn install(_app: tauri::AppHandle) {}
    pub fn current_settings() -> openflow_core::Settings {
        openflow_core::Settings::default()
    }
    pub fn update_settings(_new: openflow_core::Settings) {}
    pub fn rebuild_providers() {}
    pub fn stats_records() -> Vec<serde_json::Value> { Vec::new() }
    pub fn dictionary_entries() -> Vec<super::DictEntryDto> {
        Vec::new()
    }
    pub fn dictionary_replace(_entries: Vec<super::DictEntryDto>) {}
    pub fn stop_dictation() {}
    pub fn cancel_dictation() {}
    pub fn trigger_hands_free() {}
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub use other::{
    cancel_dictation, current_settings, dictionary_entries, dictionary_replace,
    install, rebuild_providers, stats_records, stop_dictation,
    trigger_hands_free, update_settings,
};

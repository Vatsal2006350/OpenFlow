//! OpenFlow Tauri shell.
//!
//! Runs as a macOS accessory (menu-bar) app: a tray item, a transparent
//! always-on-top Flow Bar overlay, and a hidden Hub window. This is the M0
//! skeleton — the sidecar supervisor, real state-machine bridge, and native
//! panel promotion arrive in later milestones. The overlay already listens for
//! `openflow://flowbar/state`, so the tray demo items prove the event pipeline.

mod appctx;
mod asr_model;
mod autolearn;
mod diag;
mod google_calendar;
mod hotkey;
mod jac_cleanup;
mod jac_audio_policy;
mod jac_stats;
#[cfg(target_os = "linux")]
mod linux;
mod local_llm;
mod meetings;
mod paste;
mod permissions;
mod publik;
mod system_audio;
mod stats_store;
#[cfg(target_os = "windows")]
mod win;

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

const OVERLAY_LABEL: &str = "openflow_bar";
const HUB_LABEL: &str = "main";
const MEETING_LABEL: &str = "meeting_toast";

#[derive(serde::Deserialize)]
struct JacCleanupMessage {
    role: String,
    content: String,
}

#[tauri::command]
fn register_jac_cleanup() {
    jac_cleanup::register();
}

#[cfg(target_os = "macos")]
#[tauri::command]
fn register_jac_dictation() {
    hotkey::register_jac_dictation();
}

#[cfg(target_os = "macos")]
#[tauri::command]
fn unregister_jac_dictation() {
    hotkey::unregister_jac_dictation();
}

#[cfg(target_os = "macos")]
#[tauri::command]
fn apply_jac_dictation_actions(actions: Vec<serde_json::Value>) {
    hotkey::apply_jac_dictation_actions(actions);
}

#[tauri::command]
fn unregister_jac_cleanup() {
    jac_cleanup::unregister();
}

#[tauri::command]
fn resolve_jac_cleanup(request_id: String, text: String) -> bool {
    jac_cleanup::resolve(&request_id, text)
}

#[tauri::command]
fn register_jac_audio_policy() { jac_audio_policy::register(); }

#[tauri::command]
fn unregister_jac_audio_policy() { jac_audio_policy::unregister(); }

#[tauri::command]
fn resolve_jac_audio_policy(request_id: String, accepted: bool, real_attempt: bool) -> bool {
    jac_audio_policy::resolve(&request_id, accepted, real_attempt)
}

#[tauri::command]
fn register_jac_stats() { jac_stats::register(); }

#[tauri::command]
fn unregister_jac_stats() { jac_stats::unregister(); }

#[tauri::command]
fn resolve_jac_stats_record(request_id: String, record: Option<serde_json::Value>) -> bool {
    jac_stats::resolve(&request_id, record)
}

#[cfg(target_os = "macos")]
#[tauri::command]
async fn run_cleanup_with_messages(
    app: tauri::AppHandle,
    provider: String,
    raw: String,
    max_tokens: usize,
    messages: Vec<JacCleanupMessage>,
    request_body: Option<serde_json::Value>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        hotkey::cleanup_with_messages(
            &app,
            &provider,
            &raw,
            max_tokens,
            messages.into_iter().map(|message| (message.role, message.content)).collect(),
            request_body,
        )
    })
    .await
    .map_err(|error| format!("cleanup worker failed: {error}"))?
}

// Dictation reduction still has a separate macOS native hotkey adapter.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn register_jac_dictation() {
    #[cfg(target_os = "windows")]
    win::register_jac_dictation();
    #[cfg(target_os = "linux")]
    linux::register_jac_dictation();
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn unregister_jac_dictation() {
    #[cfg(target_os = "windows")]
    win::unregister_jac_dictation();
    #[cfg(target_os = "linux")]
    linux::unregister_jac_dictation();
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
fn apply_jac_dictation_actions(actions: Vec<serde_json::Value>) {
    #[cfg(target_os = "windows")]
    win::apply_jac_dictation_actions(actions);
    #[cfg(target_os = "linux")]
    linux::apply_jac_dictation_actions(actions);
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
async fn run_cleanup_with_messages(
    app: tauri::AppHandle,
    provider: String,
    raw: String,
    max_tokens: usize,
    messages: Vec<JacCleanupMessage>,
    request_body: Option<serde_json::Value>,
) -> Result<String, String> {
    let messages = messages.into_iter().map(|message| (message.role, message.content)).collect();
    #[cfg(target_os = "linux")]
    { return linux::cleanup_with_messages(&app, &provider, &raw, max_tokens, messages, request_body); }
    #[cfg(target_os = "windows")]
    { return win::cleanup_with_messages(&app, &provider, &raw, max_tokens, messages, request_body); }
    #[allow(unreachable_code)]
    Err("Jac cleanup provider adapter is not available on this platform".into())
}

/// The overlay's size in LOGICAL points — matches `build_overlay`'s
/// `.inner_size(280.0, 84.0)`.
///
/// Deliberately a constant rather than reading `outer_size()`: that returns
/// physical pixels for whichever display the window is on *right now*, so
/// dividing it by the scale of the display we are moving TO gave a size that
/// was wrong by the ratio between them. Moving from a 1× screen to a 2× one
/// computed a pill half its real size and parked it under the Dock. The window
/// is not resizable, so its logical size never changes.
const OVERLAY_W_PT: f64 = 280.0;
const OVERLAY_H_PT: f64 = 84.0;
fn build_overlay(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    let overlay =
        WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("index.html".into()))
            .title("OpenBar")
            // Transparent margins provide room for native hover hit testing and full
            // tooltips without clipping at the bottom screen edge.
            .inner_size(OVERLAY_W_PT, OVERLAY_H_PT)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            // Let the floating control receive its first pointer enter from another
            // app without making OpenFlow the key application (which steals typing
            // focus from the field the user is dictating into).
            .focusable(false)
            .focused(false)
            .resizable(false)
            // Keep the Wispr-style action tray visible at rest. Recording and status
            // states reuse this same native overlay window.
            .visible(false)
            .build()?;
    // The transparent window must not intercept clicks across its large
    // tooltip/hit-test bounds. The native pointer tracker enables input only
    // while the cursor is on or interacting with the compact tray.
    let _ = overlay.set_ignore_cursor_events(true);
    Ok(overlay)
}

fn build_hub(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, HUB_LABEL, WebviewUrl::App("index.html".into()))
        .title("OpenFlow")
        .inner_size(920.0, 640.0)
        .min_inner_size(720.0, 480.0)
        .visible(true)
        .build()
}

fn build_meeting_toast(app: &tauri::App) -> tauri::Result<WebviewWindow> {
    let window =
        WebviewWindowBuilder::new(app, MEETING_LABEL, WebviewUrl::App("index.html".into()))
            .title("OpenFlow meeting")
            .inner_size(360.0, 132.0)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focusable(false)
            .focused(false)
            .resizable(false)
            .visible(false)
            .build()?;
    position_meeting_toast(&window, 360.0);
    Ok(window)
}

fn position_meeting_toast(window: &WebviewWindow, width: f64) {
    if let Some(monitor) = window.primary_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let x = (area.position.x as f64 / scale + area.size.width as f64 / scale - width - 16.0)
            .max(8.0);
        let y = area.position.y as f64 / scale + 18.0;
        let _ = window.set_position(tauri::LogicalPosition { x, y });
    }
}

fn position_meeting_note(window: &WebviewWindow) {
    if let Some(monitor) = window.primary_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let work_w = area.size.width as f64 / scale;
        let work_h = area.size.height as f64 / scale;
        let width = 900.0_f64.min((work_w - 40.0).max(320.0));
        let height = 800.0_f64.min((work_h - 40.0).max(400.0));
        let _ = window.set_size(tauri::LogicalSize { width, height });
        let _ = window.set_position(tauri::LogicalPosition {
            x: area.position.x as f64 / scale + work_w - width - 20.0,
            y: area.position.y as f64 / scale + (work_h - height) / 2.0,
        });
    }
    let _ = window.set_focusable(true);
    let _ = window.set_always_on_top(false);
}

pub fn show_calendar_reminder(app: &tauri::AppHandle, event: &google_calendar::CalendarEvent) {
    if meetings::is_busy() { return; }
    if let Some(window) = app.get_webview_window(MEETING_LABEL) {
        compact_meeting_note(app.clone());
        let _ = app.emit_to(MEETING_LABEL, "openflow://meeting/starting", event);
        let _ = window.show();
    }
}

#[tauri::command]
fn save_upcoming_calendar_events(events: Vec<google_calendar::CalendarEvent>) -> Result<(), String> {
    google_calendar::save_upcoming(&events)
}

#[tauri::command]
fn display_calendar_reminder(app: tauri::AppHandle, event: google_calendar::CalendarEvent) {
    show_calendar_reminder(&app, &event);
}

#[derive(Clone, Serialize)]
struct BarStatePayload {
    state: &'static str,
}

/// Emit a flow-bar state and ask the Jac overlay to place/show its window.
///
/// The single choke point every bar-state producer goes through (the macOS
/// state machine in `hotkey.rs`, the Windows pipeline in `win.rs`, and the
/// diagnostics path in `diag.rs`), so the pill's on-screen existence can
/// never drift out of sync with the state it shows.
pub fn emit_flowbar_state(app: &tauri::AppHandle, state: &'static str) {
    let _ = app.emit_to(
        OVERLAY_LABEL,
        "openflow://flowbar/state",
        BarStatePayload { state },
    );
    let _ = app.emit_to(OVERLAY_LABEL, "openflow://overlay/reposition", ());
}

/// Return raw monitor facts; Jac owns logical-point conversion and placement.
#[tauri::command]
fn get_overlay_layout_facts(window: WebviewWindow) -> Option<serde_json::Value> {
    let monitor = window
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.available_monitors().ok().and_then(|items| items.into_iter().next()))?;
    let area = monitor.work_area();
    Some(serde_json::json!({
        "monitor_y": monitor.position().y,
        "work_x": area.position.x,
        "work_y": area.position.y,
        "work_w": area.size.width,
        "work_h": area.size.height,
        "scale_factor": monitor.scale_factor(),
        "window_w": OVERLAY_W_PT,
        "window_h": OVERLAY_H_PT
    }))
}

/// Apply Jac's computed coordinates and then reveal the native overlay window.
#[tauri::command]
fn apply_overlay_position(window: WebviewWindow, x: f64, y: f64) {
    #[cfg(target_os = "macos")]
    hotkey::set_overlay_bounds(x, y, OVERLAY_W_PT, OVERLAY_H_PT);
    let _ = window.set_position(tauri::LogicalPosition { x, y });
    let _ = window.show();
}

#[tauri::command]
fn get_settings() -> serde_json::Value {
    let settings = hotkey::current_settings();
    let mut value = serde_json::to_value(&settings).unwrap_or_default();
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "_jac_missing_fields".into(),
            serde_json::to_value(&settings.jac_missing_fields).unwrap_or_default(),
        );
        object.insert(
            "_jac_platform".into(),
            serde_json::Value::String(if cfg!(target_os = "macos") {
                "macos"
            } else if cfg!(target_os = "windows") {
                "windows"
            } else {
                "linux"
            }.into()),
        );
    }
    value
}

#[tauri::command]
fn set_settings(app: tauri::AppHandle, settings: openflow_core::Settings) {
    hotkey::update_settings(settings.clone());
    let _ = app.emit("openflow://settings/updated", settings);
    // The hands-free hotkey may have changed — re-register it from the new
    // settings so a customized combo takes effect without a relaunch.
    apply_hands_free_shortcut(&app);
}

/// Stop and finalize the current recording — the overlay pill's red Stop button.
#[tauri::command]
fn stop_dictation() {
    #[cfg(target_os = "macos")]
    hotkey::stop_dictation();
    #[cfg(target_os = "windows")]
    win::stop_dictation();
    #[cfg(target_os = "linux")]
    linux::stop_dictation();
}

#[tauri::command]
fn toggle_hands_free_dictation() {
    #[cfg(target_os = "macos")]
    hotkey::trigger_hands_free();
    #[cfg(target_os = "windows")]
    win::trigger_hands_free();
    #[cfg(target_os = "linux")]
    linux::trigger_hands_free();
}

#[tauri::command]
fn open_meetings(app: tauri::AppHandle) {
    show_hub(&app);
    let _ = app.emit("openflow://open-meetings", ());
}

/// Discard the current recording — the overlay pill's ✕ button.
#[tauri::command]
fn cancel_dictation() {
    #[cfg(target_os = "macos")]
    hotkey::cancel_dictation();
    #[cfg(target_os = "windows")]
    win::cancel_dictation();
    #[cfg(target_os = "linux")]
    linux::cancel_dictation();
}

#[tauri::command]
fn get_stats_records() -> Vec<serde_json::Value> {
    hotkey::stats_records()
}

/// Clock and local timezone are environmental inputs; Jac owns aggregation.
#[tauri::command]
fn get_stats_environment() -> [i64; 2] {
    [
        chrono::Utc::now().timestamp(),
        -chrono::Local::now().offset().local_minus_utc() as i64 / 60,
    ]
}

/// Recent dictations for the Hub Home history list (newest first).
/// Local file adapter for the Jac scratchpad view.
#[tauri::command]
fn get_scratchpad(app: tauri::AppHandle) -> String {
    app.path().app_data_dir().ok()
        .and_then(|dir| std::fs::read_to_string(dir.join("scratchpad.txt")).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn save_scratchpad(app: tauri::AppHandle, text: String) -> Result<(), String> {
    let path = app.path().app_data_dir().map_err(|error| error.to_string())?.join("scratchpad.txt");
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(|error| error.to_string())?; }
    std::fs::write(path, text).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_meetings() -> Result<Vec<serde_json::Value>, String> {
    meetings::list()
}

#[tauri::command]
async fn persist_meeting_record(app: tauri::AppHandle, record: serde_json::Value) -> Result<serde_json::Value, String> {
    let saved = tauri::async_runtime::spawn_blocking(move || meetings::save_record(record))
        .await.map_err(|e| e.to_string())??;
    let _ = app.emit("openflow://meeting/saved", saved.clone());
    Ok(saved)
}

#[tauri::command]
fn start_meeting_recording(app: tauri::AppHandle, title: Option<String>) -> Result<(), String> {
    // Older note-window webviews can invoke this command before their title
    // field is populated. Treat that as an untitled note instead of failing
    // with Tauri's "missing required key title" argument error.
    let title = title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Untitled meeting".into());
    meetings::start_recording(app.clone(), title.clone())?;
    if let Some(window) = app.get_webview_window(MEETING_LABEL) {
        position_meeting_note(&window);
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit(
        "openflow://meeting/recording",
        serde_json::json!({"title":title}),
    );
    Ok(())
}

#[tauri::command]
async fn stop_meeting_recording(
    app: tauri::AppHandle,
) -> Result<String, String> {
    if let Some(window) = app.get_webview_window(MEETING_LABEL) {
        position_meeting_note(&window);
    }
    let _ = app.emit("openflow://meeting/processing", ());
    let result = tauri::async_runtime::spawn_blocking(meetings::stop_recording)
        .await.map_err(|e| e.to_string()).and_then(|result| result);
    if let Err(error) = &result {
        let _ = app.emit("openflow://meeting/error", error);
    }
    result
}

#[tauri::command]
fn compact_meeting_note(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(MEETING_LABEL) {
        let _ = window.set_size(tauri::LogicalSize {
            width: 360.0,
            height: 132.0,
        });
        let _ = window.set_focusable(false);
        let _ = window.set_always_on_top(true);
        position_meeting_toast(&window, 360.0);
    }
}

#[tauri::command]
fn expand_meeting_note(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(MEETING_LABEL) {
        position_meeting_note(&window);
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn hide_meeting_note(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(MEETING_LABEL) { let _ = window.hide(); }
}

#[tauri::command]
async fn cancel_meeting_recording(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(meetings::cancel_recording).await.map_err(|e| e.to_string())?;
    hide_meeting_note(app.clone());
    let _ = app.emit("openflow://meeting/cancelled", ());
    Ok(())
}

#[tauri::command]
fn open_meeting_url(url: String) -> Result<(), String> {
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err("Meeting link must use HTTP or HTTPS".into());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(parsed.as_str())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", parsed.as_str()])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(parsed.as_str())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn get_google_calendar_accounts() -> Vec<google_calendar::Account> {
    google_calendar::list_accounts()
}

#[tauri::command]
fn get_upcoming_calendar_events() -> Vec<google_calendar::CalendarEvent> {
    google_calendar::upcoming()
}

#[tauri::command]
async fn sync_google_calendar(time_min: String, time_max: String) -> Vec<google_calendar::CalendarFeed> {
    tauri::async_runtime::spawn_blocking(move || google_calendar::fetch_upcoming(time_min, time_max))
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn connect_google_calendar(app: tauri::AppHandle) {
    google_calendar::connect(app);
}

/// Dictionary entries for the Hub Dictionary screen.
#[tauri::command]
fn get_dictionary() -> Vec<hotkey::DictEntryDto> {
    hotkey::dictionary_entries()
}

/// Persist a complete dictionary state computed by the Jac client.
#[tauri::command]
fn save_dictionary_entries(entries: Vec<hotkey::DictEntryDto>) {
    hotkey::dictionary_replace(entries);
}

/// Permission + capability status shown in the Hub.
///
/// The permission half is a live read every time (see `permissions::snapshot`);
/// nothing here is remembered between calls. The Hub no longer has to ask for it
/// on a timer either — `permissions::watch` pushes the same shape at it on
/// `openflow://permissions` the moment macOS changes its mind.
#[derive(Clone, Serialize)]
struct StatusReport {
    accessibility: bool,
    microphone: bool,
    input_monitoring: bool,
    microphone_grant: permissions::Grant,
    charged_to: Option<String>,
    has_openai_key: bool,
    has_anthropic_key: bool,
    /// A publik API key is present (env, keychain, or the convention file).
    has_publik_key: bool,
    /// The on-device cleanup worker AND a model are on disk — when false,
    /// "Local" pastes the transcript as spoken and Settings says so.
    local_model_present: bool,
}

#[tauri::command]
fn get_status() -> StatusReport {
    let p = permissions::snapshot();
    StatusReport {
        accessibility: p.accessibility,
        microphone: p.microphone,
        input_monitoring: p.input_monitoring,
        microphone_grant: p.microphone_grant,
        charged_to: p.charged_to,
        has_openai_key: has_key("openai_api_key"),
        has_anthropic_key: has_key("anthropic_api_key"),
        has_publik_key: publik::read_key().is_some(),
        local_model_present: local_llm::worker_bin_path().is_some()
            && local_llm::model_path().exists(),
    }
}

/// The most recent loud diagnostic (permission/injection failure), if any —
/// lets the Hub show what went wrong even if it was opened after the fact.
/// See `diag::report`, called from the dictation pipeline whenever text
/// fails to reach the cursor.
#[tauri::command]
fn get_last_error() -> Option<diag::ErrorDto> {
    diag::last_error()
}

#[tauri::command]
fn report_jac_diagnostic(app: tauri::AppHandle, code: String, headline: String, detail: String) {
    diag::present(&app, code, headline, detail);
}

#[tauri::command]
fn set_flowbar_idle(app: tauri::AppHandle) {
    emit_flowbar_state(&app, "idle");
}

#[tauri::command]
fn asr_platform() -> &'static str {
    #[cfg(target_os = "macos")]
    { "macos" }
    #[cfg(target_os = "windows")]
    { "windows" }
    #[cfg(target_os = "linux")]
    { "linux" }
}

#[tauri::command]
fn initialize_asr_model(app: tauri::AppHandle, fields: Vec<String>) -> Result<(), String> {
    asr_model::start(&app, fields)
}

#[tauri::command]
fn focus_model_prompt(app: tauri::AppHandle) {
    asr_model::focus_model_prompt(&app);
}

/// The keychain account for a BYO provider; anything else is refused.
fn byo_account(provider: &str) -> Result<&'static str, String> {
    match provider {
        "openai" => Ok("openai_api_key"),
        "anthropic" => Ok("anthropic_api_key"),
        _ => Err(format!("unknown provider {provider}")),
    }
}

fn has_key(account: &str) -> bool {
    keyring::Entry::new("com.openflow.openflow", account)
        .ok()
        .and_then(|e| e.get_password().ok())
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

#[cfg(target_os = "linux")]
fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(target_os = "windows")]
fn open_url(url: &str) {
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();
}

/// Request microphone access: on macOS, trigger the native AVFoundation prompt so
/// the bundle registers in Privacy & Security, then open the Microphone settings
/// pane; on Linux, briefly open the input device (there is no permission prompt)
/// and open pavucontrol as the closest equivalent.
#[tauri::command]
fn request_microphone() {
    #[cfg(target_os = "macos")]
    {
        paste::request_microphone_access();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
    }
    #[cfg(target_os = "linux")]
    {
        // On Linux, microphone access is controlled by PipeWire/PulseAudio.
        // Opening pavucontrol is the closest equivalent to macOS's permission pane.
        std::thread::spawn(|| {
            if let Ok(h) = openflow_audio::start(|_: &[f32]| {}) {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let _ = h.stop();
            }
        });
        let _ = std::process::Command::new("pavucontrol").spawn();
    }
}

/// Request Accessibility — the permission that makes the Fn key work in every app and
/// lets us type into other apps. Fire the native prompt, then open the pane.
#[tauri::command]
fn request_accessibility() {
    #[cfg(target_os = "macos")]
    {
        let _ = paste::prompt_accessibility();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
    }
    #[cfg(target_os = "linux")]
    {
        // On Linux, keyboard input access is via the 'input' group.
        // Open the system settings or print instructions.
        eprintln!("[openflow] Linux: ensure your user is in the 'input' group: sudo usermod -aG input $USER");
        let _ = open_url("https://wiki.archlinux.org/title/Input_device");
    }
}

/// Request Input Monitoring (needed for the Fn key to be seen in every app, not
/// just while OpenFlow is frontmost): register + prompt, then open the pane.
#[tauri::command]
fn request_input_monitoring() {
    #[cfg(target_os = "macos")]
    {
        let _ = paste::request_input_monitoring();
        open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent");
    }
    #[cfg(target_os = "linux")]
    {
        eprintln!(
            "[openflow] Linux: input monitoring is handled via rdev (needs 'input' group membership)"
        );
    }
}

/// Save (or clear, when empty) an API key in the OS keychain, then rebuild providers
/// so it takes effect immediately. Accepts exactly the two BYO providers: no UI
/// path can write a publik key here or overwrite a user key with one
/// (`publik::ACCOUNT` is managed only by `publik.rs`).
#[tauri::command]
fn set_api_key(provider: String, key: String) -> Result<(), String> {
    let account = byo_account(&provider)?;
    let entry = keyring::Entry::new("com.openflow.openflow", account).map_err(|e| e.to_string())?;
    let key = key.trim();
    // Delete any existing item first so the new one is created by (and readable to)
    // this app — a key added via the `security` CLI isn't readable by the app.
    let _ = entry.delete_credential();
    if !key.is_empty() {
        entry.set_password(key).map_err(|e| e.to_string())?;
    }
    hotkey::rebuild_providers();
    Ok(())
}

/// Show and focus the Hub window, whether it's hidden (closed via the X button,
/// which we intercept below) or just needs to come to the front. Used by both
/// the tray's "Open OpenFlow" item and a relaunch caught by the single-instance
/// guard (Windows/Linux).
fn show_hub(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window(HUB_LABEL) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// (Re)register the customizable hands-free global hotkey from the current
/// settings — press once to start hands-free dictation, again to stop. Called at
/// startup and whenever settings change. Best-effort: an unregisterable or empty
/// accelerator just leaves the hotkey off (Fn push-to-talk and double-tap-Fn
/// hands-free still work), never a crash.
fn apply_hands_free_shortcut(app: &tauri::AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let global_shortcut = app.global_shortcut();
    let _ = global_shortcut.unregister_all();
    let accelerator = hotkey::current_settings().hands_free_hotkey;
    if accelerator.trim().is_empty() {
        return;
    }
    match global_shortcut.register(accelerator.as_str()) {
        Ok(()) => eprintln!("[openflow] hands-free hotkey registered: {accelerator}"),
        Err(e) => eprintln!(
            "[openflow] hands-free hotkey '{accelerator}' could not be registered; another app may own it: {e}"
        ),
    }
}

pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();
    // Relaunching the app (double-clicking the exe/installer shortcut again) must
    // not spawn a second process — it should just surface the running one. Without
    // this, every relaunch left a new instance in the taskbar. macOS's Dock already
    // re-activates the existing instance, so this is Windows/Linux-only.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_hub(app);
        }));
    }
    builder
        .plugin(
            // The customizable hands-free hotkey lives here — the OS registers the
            // chord, so pressing it fires our handler AND is suppressed from the
            // focused app (a listen-only CGEvent tap could not consume a printable
            // key like Space). Only the hands-free shortcut is ever registered, so
            // any Pressed event is a hands-free toggle.
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|_app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        hotkey::trigger_hands_free();
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            stop_dictation,
            toggle_hands_free_dictation,
            open_meetings,
            cancel_dictation,
            get_stats_records,
            get_stats_environment,
            get_scratchpad,
            save_scratchpad,
            list_meetings,
            persist_meeting_record,
            start_meeting_recording,
            stop_meeting_recording,
            compact_meeting_note,
            expand_meeting_note,
            hide_meeting_note,
            cancel_meeting_recording,
            open_meeting_url,
            get_google_calendar_accounts,
            get_upcoming_calendar_events,
            sync_google_calendar,
            save_upcoming_calendar_events,
            display_calendar_reminder,
            connect_google_calendar,
            get_dictionary,
            save_dictionary_entries,
            get_status,
            get_overlay_layout_facts,
            apply_overlay_position,
            get_last_error,
            report_jac_diagnostic,
            set_flowbar_idle,
            asr_model::download_asr_model,
            asr_platform,
            initialize_asr_model,
            focus_model_prompt,
            request_microphone,
            request_accessibility,
            request_input_monitoring,
            set_api_key,
            publik::get_publik_status,
            publik::publik_refresh_wallet,
            publik::publik_accept_disclosure,
            publik::publik_forget_key,
            publik::publik_dismiss_first_run,
            publik::publik_dismiss_notice,
            publik::publik_open_url,
            register_jac_cleanup,
            unregister_jac_cleanup,
            resolve_jac_cleanup,
            register_jac_audio_policy,
            unregister_jac_audio_policy,
            resolve_jac_audio_policy,
            register_jac_stats,
            unregister_jac_stats,
            resolve_jac_stats_record,
            run_cleanup_with_messages,
            register_jac_dictation,
            unregister_jac_dictation,
            apply_jac_dictation_actions,
        ])
        .setup(|app| {
            // Regular app: shows in the Dock with a normal, focusable main window.
            // (Can switch to a menu-bar-only accessory app later for the Wispr look.)
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Regular);

            build_overlay(app)?;
            build_meeting_toast(app)?;
            let hub = build_hub(app)?;
            let _ = hub.show();
            let _ = hub.set_focus();
            // Closing the Hub via the X button should hide it (dictation keeps
            // running from the tray), not destroy the window — otherwise "Open
            // OpenFlow" in the tray has nothing left to show. macOS already
            // keeps the app (and its Dock icon) alive on window close, so this
            // matters most on Windows/Linux, but is harmless everywhere.
            hub.on_window_event({
                let app_handle = app.handle().clone();
                move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window(HUB_LABEL) {
                            let _ = w.hide();
                        }
                    }
                }
            });

            // Wire the Fn key to the pill via the real state machine.
            hotkey::install(app.handle().clone());

            // Register the customizable hands-free hotkey (default Cmd+Shift+Space).
            apply_hands_free_shortcut(app.handle());

            // Keep the permission rows honest without the Hub having to be awake
            // to ask. This is what makes the setup screen's promise ("turns green
            // the moment macOS applies it — no relaunch needed") actually true:
            // the Hub's own timer stops within seconds of its window going away,
            // and the reader is granting from System Settings precisely then.
            permissions::watch(app.handle().clone());

            let open = MenuItem::with_id(app, "open", "Open OpenFlow", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit OpenFlow", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &sep, &quit])?;

            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_hub(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.build(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running OpenFlow");
}

#[cfg(test)]
mod tests {
    use super::byo_account;

    #[test]
    fn set_api_key_still_rejects_unknown_providers() {
        // The BYO command accepts exactly openai/anthropic — "publik" is not a
        // way to write (or clobber) a key through it.
        assert_eq!(byo_account("openai"), Ok("openai_api_key"));
        assert_eq!(byo_account("anthropic"), Ok("anthropic_api_key"));
        assert!(byo_account("publik").is_err());
        assert!(byo_account("").is_err());
    }
}

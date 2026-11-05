//! The Whisper speech model: where it lives, loading it, and fetching it.
//!
//! v0.2.1 shipped as a 4.9 MB dmg with no speech model inside, and the only
//! remedy it offered was a banner telling the reader to fetch a file from
//! Hugging Face by hand, put it in a hidden folder, and relaunch. For anyone
//! who installed from the dmg that is a dead end. Two fixes live here:
//!
//! 1. A release build carries `ggml-base.en.bin` inside the app
//!    (`Contents/Resources/models/`, placed there by scripts/build-macos.sh).
//!    It is the last candidate, so a bigger model the reader added still wins.
//! 2. When no usable model is found anyway (a source build, a deleted or
//!    damaged file), the Hub shows one button. It downloads the model into the
//!    models folder, checks its size and SHA-256, and loads it on the spot —
//!    no relaunch.
//!
//! Both platform layers (`hotkey.rs` on macOS, `win.rs` on Windows) read the
//! engine through [`engine`] instead of each keeping their own copy.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

const INPUT_EVENT: &str = "openflow://asr-model/input";

/// Model selection and pinned download metadata supplied by the Jac policy.
#[derive(Clone, Debug)]
struct ModelPolicy {
    default_file: String,
    url: String,
    sha256: String,
    bytes: u64,
    candidates: Vec<String>,
}

impl ModelPolicy {
    fn from_fields(fields: Vec<String>) -> Result<Self, String> {
        if fields.len() < 5 {
            return Err("Jac supplied an incomplete speech-model policy".into());
        }
        let default_file = fields[0].clone();
        let url = fields[1].clone();
        let sha256 = fields[2].clone();
        let bytes = fields[3]
            .parse::<u64>()
            .map_err(|_| "Jac supplied an invalid model size".to_string())?;
        let candidates = fields[4..].to_vec();
        if bytes == 0 || sha256.len() != 64 || candidates.is_empty() {
            return Err("Jac supplied invalid speech-model metadata".into());
        }
        if candidates.iter().any(|name| Path::new(name).file_name().and_then(|part| part.to_str()) != Some(name)) {
            return Err("Jac supplied an invalid speech-model filename".into());
        }
        Ok(Self { default_file, url, sha256, bytes, candidates })
    }
}

static ENGINE: OnceLock<Arc<openflow_asr::WhisperEngine>> = OnceLock::new();
static BUNDLED_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
static POLICY: OnceLock<ModelPolicy> = OnceLock::new();
static DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// The per-user models folder. It survives app updates and reinstalls.
pub fn models_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var("APPDATA").unwrap_or_default();
        PathBuf::from(base).join("OpenFlow").join("models")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/OpenFlow/models")
    }
}

/// Every model file on disk, in the order to try them: the reader's own
/// models (best first), then the copy bundled in the app. A file that fails
/// to load is skipped, so one damaged download can't block a good fallback.
fn candidates_in(
    user_dir: &Path,
    bundled_dir: Option<&Path>,
    candidate_names: &[String],
    default_file: &str,
) -> Vec<(PathBuf, bool)> {
    let mut out: Vec<(PathBuf, bool)> = candidate_names
        .iter()
        .map(|name| user_dir.join(name))
        .filter(|p| p.is_file())
        .map(|p| (p, false))
        .collect();
    if let Some(dir) = bundled_dir {
        let p = dir.join(default_file);
        if p.is_file() {
            out.push((p, true));
        }
    }
    out
}

fn bundled_dir() -> Option<PathBuf> {
    BUNDLED_DIR.get().cloned().flatten()
}

/// The loaded engine, or `None` while loading or when no model is available.
pub fn engine() -> Option<Arc<openflow_asr::WhisperEngine>> {
    ENGINE.get().cloned()
}

fn emit_model_input(app: &AppHandle, input: Vec<String>) {
    let _ = app.emit(INPUT_EVENT, input);
}

/// Find and load the Jac-selected model candidates off the main thread.
pub fn start(app: &AppHandle, fields: Vec<String>) -> Result<(), String> {
    let policy = ModelPolicy::from_fields(fields)?;
    POLICY
        .set(policy.clone())
        .map_err(|_| "Speech-model initialization already ran".to_string())?;
    let bundled = app.path().resource_dir().ok().map(|d| d.join("models"));
    let _ = BUNDLED_DIR.set(bundled);
    let app = app.clone();
    std::thread::spawn(move || {
        let found = candidates_in(
            &models_dir(),
            bundled_dir().as_deref(),
            &policy.candidates,
            &policy.default_file,
        );
        for (path, bundled) in &found {
            if load(&app, path, *bundled) {
                return;
            }
        }
        eprintln!(
            "[openflow] no usable speech model in {} or the app bundle",
            models_dir().display()
        );
        emit_model_input(&app, vec!["missing".into()]);
    });
    Ok(())
}

fn load(app: &AppHandle, path: &Path, bundled: bool) -> bool {
    match openflow_asr::WhisperEngine::load(path) {
        Ok(engine) => {
            let _ = ENGINE.set(Arc::new(engine));
            let file = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            eprintln!(
                "[openflow] ASR model loaded ({}) — ready to transcribe",
                path.display()
            );
            emit_model_input(app, vec!["ready".into(), file, bundled.to_string()]);
            crate::diag::clear_if("asr_unavailable");
            true
        }
        Err(e) => {
            eprintln!(
                "[openflow] ASR model at {} failed to load: {e}",
                path.display()
            );
            false
        }
    }
}

/// A dictation finished with no engine to transcribe it. Say why, and when
/// the fix is the download, bring the Hub forward with the download popup.
pub fn report_unavailable(app: &AppHandle) {
    emit_model_input(
        app,
        vec![
            "unavailable".into(),
            ENGINE.get().is_some().to_string(),
            DOWNLOAD_IN_PROGRESS.load(Ordering::Acquire).to_string(),
        ],
    );
}

/// Bring the Hub forward when the Jac model policy decides to show its prompt.
pub fn focus_model_prompt(app: &AppHandle) {
    if let Some(hub) = app.get_webview_window("main") {
        let _ = hub.show();
        let _ = hub.unminimize();
        let _ = hub.set_focus();
    }
}

/// Start a Jac-selected model download when the inference engine is absent.
#[tauri::command]
pub fn download_asr_model(app: AppHandle) -> bool {
    let Some(policy) = POLICY.get().cloned() else { return false; };
    if ENGINE.get().is_some() || DOWNLOAD_IN_PROGRESS.swap(true, Ordering::AcqRel) {
        return false;
    }
    emit_model_input(&app, vec!["download_started".into(), policy.bytes.to_string()]);

    std::thread::spawn(move || {
        let dir = models_dir();
        let part = dir.join(format!("{}.part", policy.default_file));
        let result = fetch(&dir, &part, &policy, |received, total| {
            emit_model_input(
                &app,
                vec!["download_progress".into(), received.to_string(), total.to_string()],
            )
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        match result {
            Ok(path) => {
                if !load(&app, &path, false) {
                    emit_model_input(&app, vec!["download_load_failed".into()]);
                }
            }
            Err(message) => {
                eprintln!("[openflow] speech model download failed: {message}");
                emit_model_input(&app, vec!["download_failed".into(), message]);
            }
        }
        DOWNLOAD_IN_PROGRESS.store(false, Ordering::Release);
    });
    true
}

/// Download into `part`, check size and SHA-256, then move it into place as
/// the Jac-selected default file. `progress` gets (received, total) about 5 times a
/// second and once at the end.
fn fetch(
    dir: &Path,
    part: &Path,
    policy: &ModelPolicy,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| save_message(&e))?;

    // The blocking client applies this timeout to each read, not to the whole
    // body, so a slow 148 MB download is fine but a stalled one ends.
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("OpenFlow/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("download_client_failed:{e}"))?;
    let mut resp = client
        .get(&policy.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| network_message(&e))?;
    let total = resp.content_length().unwrap_or(policy.bytes);

    let mut file = std::fs::File::create(part).map_err(|e| save_message(&e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut received: u64 = 0;
    let mut last_emit = Instant::now();
    loop {
        let n = resp.read(&mut buf).map_err(|_| "download_interrupted".to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| save_message(&e))?;
        hasher.update(&buf[..n]);
        received += n as u64;
        if last_emit.elapsed() >= Duration::from_millis(200) {
            progress(received, total);
            last_emit = Instant::now();
        }
    }
    file.sync_all().map_err(|e| save_message(&e))?;
    drop(file);
    progress(received, total);

    let digest = to_hex(&hasher.finalize());
    if received != policy.bytes || digest != policy.sha256 {
        return Err("checksum_mismatch".into());
    }
    let dest = dir.join(&policy.default_file);
    std::fs::rename(part, &dest).map_err(|e| save_message(&e))?;
    Ok(dest)
}

fn network_message(e: &reqwest::Error) -> String {
    match e.status() {
        Some(status) => format!("server_status:{}", status.as_u16()),
        None => "network_unreachable".into(),
    }
}

fn save_message(e: &std::io::Error) -> String {
    format!("save_failed:{e}")
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FILE: &str = "ggml-base.en.bin";
    const TEST_BYTES: u64 = 147_964_211;
    const TEST_SHA: &str = "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002";

    fn test_policy() -> ModelPolicy {
        ModelPolicy {
            default_file: TEST_FILE.into(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin".into(),
            sha256: TEST_SHA.into(),
            bytes: TEST_BYTES,
            candidates: vec!["ggml-small.en.bin".into(), TEST_FILE.into()],
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("openflow-asr-model-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_model_anywhere_means_no_candidates() {
        let user = scratch("none-user");
        let bundled = scratch("none-bundled");
        let policy = test_policy();
        assert!(candidates_in(&user, Some(&bundled), &policy.candidates, TEST_FILE).is_empty());
        assert!(candidates_in(&user, None, &policy.candidates, TEST_FILE).is_empty());
    }

    #[test]
    fn the_bundled_copy_is_used_when_the_user_has_none() {
        let user = scratch("bundled-user");
        let bundled = scratch("bundled-app");
        std::fs::write(bundled.join(TEST_FILE), b"x").unwrap();
        let policy = test_policy();
        let found = candidates_in(&user, Some(&bundled), &policy.candidates, TEST_FILE);
        assert_eq!(found, vec![(bundled.join(TEST_FILE), true)]);
    }

    #[test]
    fn a_model_the_user_added_wins_over_the_bundled_copy() {
        let user = scratch("pref-user");
        let bundled = scratch("pref-app");
        std::fs::write(bundled.join(TEST_FILE), b"x").unwrap();
        std::fs::write(user.join("ggml-small.en.bin"), b"x").unwrap();
        std::fs::write(user.join(TEST_FILE), b"x").unwrap();
        let policy = test_policy();
        let found = candidates_in(&user, Some(&bundled), &policy.candidates, TEST_FILE);
        assert_eq!(
            found,
            vec![
                (user.join("ggml-small.en.bin"), false),
                (user.join(TEST_FILE), false),
                (bundled.join(TEST_FILE), true),
            ]
        );
    }

    #[test]
    fn a_leftover_partial_download_is_not_a_candidate() {
        let user = scratch("part-user");
        std::fs::write(user.join(format!("{TEST_FILE}.part")), b"x").unwrap();
        let policy = test_policy();
        assert!(candidates_in(&user, None, &policy.candidates, TEST_FILE).is_empty());
    }

    #[test]
    fn hex_matches_the_pinned_checksum_format() {
        assert_eq!(to_hex(&[0x00, 0xab, 0x0f]), "00ab0f");
        assert_eq!(TEST_SHA.len(), 64);
        assert_eq!(
            to_hex(&Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// The real download, end to end: 148 MB from huggingface.co, checksum,
    /// rename, and a Whisper load of the result. Network, so opt-in:
    /// `cargo test --release -p openflow-tauri real_download -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_download_verifies_and_loads() {
        let dir = scratch("real-download");
        let policy = test_policy();
        let part = dir.join(format!("{}.part", policy.default_file));
        let mut calls = 0u32;
        let mut last = (0u64, 0u64);
        let path = fetch(&dir, &part, &policy, |r, t| {
            calls += 1;
            last = (r, t);
        })
        .expect("download");
        assert_eq!(path, dir.join(TEST_FILE));
        assert!(!part.exists(), "the .part file is renamed away");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), TEST_BYTES);
        assert_eq!(last, (TEST_BYTES, TEST_BYTES));
        assert!(
            calls > 2,
            "progress was reported during the download ({calls} calls)"
        );
        openflow_asr::WhisperEngine::load(&path).expect("the downloaded model loads");
        eprintln!("downloaded, verified, loaded; {calls} progress calls");
        let _ = std::fs::remove_dir_all(&dir);
    }

}

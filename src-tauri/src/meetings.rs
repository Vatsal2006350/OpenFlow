//! Private local meeting library shared by the Hub and the stdio MCP server.
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}},
    thread::{self, JoinHandle},
    time::Instant,
};
use tauri::Emitter;

struct Recording {
    handle: openflow_audio::CaptureHandle,
    system_audio: crate::system_audio::SystemAudioHandle,
    audio_tx: crate::system_audio::AudioSender,
    worker: JoinHandle<Vec<f32>>,
    stopped: Arc<AtomicBool>,
}
static STORE_LOCK: Mutex<()> = Mutex::new(());
static PROCESSING: AtomicBool = AtomicBool::new(false);

pub fn is_busy() -> bool {
    PROCESSING.load(Ordering::SeqCst) || RECORDING.get().and_then(|slot| slot.lock().ok().map(|slot| slot.is_some())).unwrap_or(false)
}

static RECORDING: OnceLock<Mutex<Option<Recording>>> = OnceLock::new();

pub fn start_recording(app: tauri::AppHandle, title: String) -> Result<(), String> {
    if PROCESSING.load(Ordering::SeqCst) { return Err("The previous meeting is still being saved.".into()); }
    if crate::asr_model::engine().is_none() {
        return Err(
            "Local Whisper is not ready. Download the speech model in Settings, then try again."
                .into(),
        );
    }
    let slot = RECORDING.get_or_init(|| Mutex::new(None));
    let mut active = slot.lock().map_err(|e| e.to_string())?;
    if active.is_some() {
        return Err("A meeting recording is already in progress".into());
    }
    let (audio_tx, audio_rx) = mpsc::channel::<crate::system_audio::AudioChunk>();
    let callback_tx = audio_tx.clone();
    let started = Instant::now();
    let handle = openflow_audio::start_with_audio(
        |_| {},
        move |samples| {
            let _ = callback_tx.send(crate::system_audio::AudioChunk {
                source: 0,
                at: Instant::now(),
                samples: samples.to_vec(),
            });
        },
    )
    .map_err(|e| e.to_string())?;
    let sample_rate = handle.sample_rate();
    let system_audio = crate::system_audio::SystemAudioHandle::start(audio_tx.clone())?;
    let worker_app = app.clone();
    let worker_title = title.clone();
    let stopped = Arc::new(AtomicBool::new(false));
    let worker_stopped = stopped.clone();
    let worker = thread::Builder::new().name("openflow-live-transcription".into()).spawn(move || {
        let mut sources = [Vec::<f32>::new(), Vec::<f32>::new()];
        let mut last_emit = 0usize;
        while let Ok(chunk) = audio_rx.recv() {
            let pcm = if chunk.source == 0 {
                openflow_audio::resample_to_16k(&chunk.samples, sample_rate)
            } else { chunk.samples };
            let offset = chunk.at.saturating_duration_since(started).as_micros() as usize * 16_000 / 1_000_000;
            let end = offset.saturating_add(pcm.len());
            let source = &mut sources[chunk.source.min(1)];
            if source.len() < end { source.resize(end, 0.0); }
            source[offset..end].copy_from_slice(&pcm);
            let total_samples = sources[0].len().max(sources[1].len());
            if worker_stopped.load(Ordering::SeqCst) { continue; }
            // Re-decode a bounded rolling window every ~3 seconds. This works
            // with the existing local Whisper model and never sends audio away.
            if total_samples.saturating_sub(last_emit) >= 48_000 {
                last_emit = total_samples;
                let mixed = mix_sources(&sources);
                let start = mixed.len().saturating_sub(12 * 16_000);
                let rolling = &mixed[start..];
                if rolling.len() >= 16_000 {
                    if let Some(engine) = crate::asr_model::engine() {
                        let text = if contains_speech_energy(rolling) { engine.transcribe_long(rolling).map(|t| t.text).unwrap_or_default() } else { String::new() };
                        if worker_stopped.load(Ordering::SeqCst) { continue; }
                        let _ = worker_app.emit("openflow://meeting/live-transcript", serde_json::json!({"title":worker_title,"rolling_text":text,"seconds":total_samples/16_000}));
                    }
                }
            }
        }
        mix_sources(&sources)
    }).map_err(|e| e.to_string())?;
    *active = Some(Recording {
        handle,
        system_audio,
        audio_tx,
        worker,
        stopped,
    });
    Ok(())
}

fn mix_sources(sources: &[Vec<f32>; 2]) -> Vec<f32> {
    let length = sources[0].len().max(sources[1].len());
    let mut mixed = Vec::with_capacity(length);
    for index in 0..length {
        let mic = sources[0].get(index).copied().unwrap_or(0.0);
        let system = sources[1].get(index).copied().unwrap_or(0.0);
        mixed.push(((mic + system) * 0.7).clamp(-1.0, 1.0));
    }
    mixed
}

fn contains_speech_energy(samples: &[f32]) -> bool {
    if samples.len() < 4000 { return false; }
    let peak = samples.iter().fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt();
    peak >= 0.005 && (rms >= 0.002 || peak >= 0.02)
}

pub fn stop_recording() -> Result<String, String> {
    if PROCESSING.swap(true, Ordering::SeqCst) { return Err("This meeting is already being saved.".into()); }
    struct FinishGuard;
    impl Drop for FinishGuard { fn drop(&mut self) { PROCESSING.store(false, Ordering::SeqCst); } }
    let _finish_guard = FinishGuard;
    let recording = RECORDING
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or_else(|| "No meeting recording is in progress".to_string())?;
    recording.stopped.store(true, Ordering::SeqCst);
    let _captured_mic = recording
        .handle
        .stop()
        .ok_or_else(|| "Microphone capture ended unexpectedly".to_string())?;
    recording.system_audio.stop();
    drop(recording.audio_tx);
    let pcm = recording.worker.join().map_err(|_| "Meeting audio capture worker stopped unexpectedly".to_string())?;
    let engine = crate::asr_model::engine().ok_or_else(|| {
        "Local Whisper is not ready. Download the speech model, then try again.".to_string()
    })?;
    if !contains_speech_energy(&pcm) { return Err("No speech was detected. The empty recording was not saved.".into()); }
    Ok(engine
        .transcribe_long(&pcm)
        .map_err(|e| format!("Local transcription failed: {e}"))?
        .text)
}

pub fn cancel_recording() {
    if let Some(slot) = RECORDING.get() {
        if let Ok(mut active) = slot.lock() {
            if let Some(recording) = active.take() {
                recording.stopped.store(true, Ordering::SeqCst);
                drop(recording.handle);
                recording.system_audio.stop();
                drop(recording.audio_tx);
                let _ = recording.worker.join();
            }
        }
    }
}

pub fn store_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    let root = PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join("Library/Application Support/OpenFlow");
    #[cfg(target_os = "windows")]
    let root = PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("OpenFlow");
    #[cfg(target_os = "linux")]
    let root =
        PathBuf::from(std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            format!("{}/.local/share", std::env::var("HOME").unwrap_or_default())
        }))
        .join("OpenFlow");
    root.join("meetings.json")
}
pub fn list() -> Result<Vec<Value>, String> {
    let path = store_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&data).map_err(|e| e.to_string())
}
pub fn save_record(record: Value) -> Result<Value, String> {
    if !record.is_object() {
        return Err("Meeting record must be a JSON object".into());
    }
    let _store_guard = STORE_LOCK.lock().map_err(|e| e.to_string())?;
    let path = store_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut meetings = list()?;
    meetings.push(record.clone());
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(&meetings).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())?;
    Ok(record)
}

use std::{sync::mpsc, time::Instant};

pub struct AudioChunk {
    pub source: usize,
    pub at: Instant,
    pub samples: Vec<f32>,
}

pub type AudioSender = mpsc::Sender<AudioChunk>;

pub struct SystemAudioHandle {
    #[cfg(target_os = "macos")]
    context: *mut AudioBridge,
}

#[cfg(target_os = "macos")]
struct AudioBridge {
    sender: AudioSender,
}

#[cfg(target_os = "macos")]
unsafe impl Send for SystemAudioHandle {}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn openflow_system_audio_start(
        callback: extern "C" fn(*const f32, i32, *mut std::ffi::c_void),
        context: *mut std::ffi::c_void,
    ) -> i32;
    fn openflow_system_audio_stop();
    fn openflow_system_audio_last_error(buffer: *mut std::ffi::c_char, capacity: i32);
}

#[cfg(target_os = "macos")]
extern "C" fn receive_system_audio(samples: *const f32, count: i32, context: *mut std::ffi::c_void) {
    if samples.is_null() || count <= 0 || context.is_null() { return; }
    // SAFETY: the Swift capture object retains this boxed context until stopCapture completes.
    let bridge = unsafe { &*(context.cast::<AudioBridge>()) };
    // SAFETY: ScreenCaptureKit holds the sample buffer for the duration of this callback.
    let samples = unsafe { std::slice::from_raw_parts(samples, count as usize) };
    let _ = bridge.sender.send(AudioChunk {
        source: 1,
        at: Instant::now(),
        samples: samples.to_vec(),
    });
}

impl SystemAudioHandle {
    pub fn start(sender: AudioSender) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            let context = Box::into_raw(Box::new(AudioBridge { sender }));
            // SAFETY: callback/context ABI is declared by the bundled Swift bridge.
            let result = unsafe { openflow_system_audio_start(receive_system_audio, context.cast()) };
            if result == 0 { return Ok(Self { context }); }
            let mut error = [0i8; 512];
            // SAFETY: writable buffer has the advertised capacity.
            unsafe { openflow_system_audio_last_error(error.as_mut_ptr(), error.len() as i32) };
            // SAFETY: the Swift bridge writes a NUL-terminated string into the buffer.
            let detail = unsafe { std::ffi::CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
            // SAFETY: start failure does not retain the context.
            unsafe { drop(Box::from_raw(context)); }
            let detail = if detail.is_empty() { format!("ScreenCaptureKit error {result}") } else { detail.into_owned() };
            return Err(format!("Could not capture incoming meeting audio. Allow OpenFlow under System Settings → Privacy & Security → Screen & System Audio Recording, then quit and reopen the app. ({detail})"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = sender;
            Ok(Self {})
        }
    }

    pub fn stop(self) {}
}

impl Drop for SystemAudioHandle {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: stops the stream and waits for callbacks before freeing context.
            unsafe { openflow_system_audio_stop(); }
            // SAFETY: this handle is the sole owner of the context box.
            if !self.context.is_null() {
                unsafe { drop(Box::from_raw(self.context)); }
                self.context = std::ptr::null_mut();
            }
        }
    }
}

//! Auto-learn: after OpenFlow pastes dictated text, watch the focused text field
//! for a few seconds. If the user corrects a single distinctive word (typically a
//! mis-heard name), diff it out and add it to the dictionary — so next time ASR/
//! cleanup spell it right. This is the signal source Wispr's ✨ sparkle needs.
//!
//! It is deliberately conservative: it only learns on a clean one-word substitution
//! into an otherwise-unchanged field, where the new word looks like a proper noun
//! and is phonetically close to the word it replaced. That avoids poisoning the
//! dictionary with common-word edits. Reads use the Accessibility API and only run
//! when Accessibility is granted.

#[cfg(target_os = "macos")]
mod imp {
    use std::os::raw::{c_char, c_void};
    use std::ptr;
    use std::time::Duration;
    use tauri::Emitter;

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type AXUIElementRef = *const c_void;

    const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    /// How long to wait after paste before checking for a correction.
    const OBSERVE_DELAY: Duration = Duration::from_secs(7);

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            cstr: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetLength(s: CFStringRef) -> isize;
        fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: isize, encoding: u32)
            -> bool;
        fn CFStringGetMaximumSizeForEncoding(len: isize, encoding: u32) -> isize;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
    }

    fn make_cfstring(s: &str) -> CFStringRef {
        let Ok(c) = std::ffi::CString::new(s) else {
            return ptr::null();
        };
        unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), KCF_STRING_ENCODING_UTF8) }
    }

    /// Convert a CFStringRef to a Rust String (None if it isn't actually a string).
    unsafe fn cfstring_to_string(s: CFStringRef) -> Option<String> {
        if s.is_null() || CFGetTypeID(s) != CFStringGetTypeID() {
            return None;
        }
        let len = CFStringGetLength(s);
        let max = CFStringGetMaximumSizeForEncoding(len, KCF_STRING_ENCODING_UTF8) + 1;
        if max <= 0 {
            return Some(String::new());
        }
        let mut buf = vec![0i8; max as usize];
        if CFStringGetCString(s, buf.as_mut_ptr(), max, KCF_STRING_ENCODING_UTF8) {
            std::ffi::CStr::from_ptr(buf.as_ptr())
                .to_str()
                .ok()
                .map(|x| x.to_string())
        } else {
            None
        }
    }

    /// Copy the system-wide focused UI element (retained — caller CFReleases it).
    unsafe fn copy_focused_element() -> AXUIElementRef {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return ptr::null();
        }
        let attr = make_cfstring("AXFocusedUIElement");
        let mut focused: CFTypeRef = ptr::null();
        let err = AXUIElementCopyAttributeValue(system, attr, &mut focused);
        if !attr.is_null() {
            CFRelease(attr);
        }
        CFRelease(system);
        if err != 0 {
            return ptr::null();
        }
        focused as AXUIElementRef
    }

    /// Read a text element's AXValue as a string.
    unsafe fn element_value(element: AXUIElementRef) -> Option<String> {
        if element.is_null() {
            return None;
        }
        let attr = make_cfstring("AXValue");
        let mut value: CFTypeRef = ptr::null();
        let err = AXUIElementCopyAttributeValue(element, attr, &mut value);
        if !attr.is_null() {
            CFRelease(attr);
        }
        if err != 0 || value.is_null() {
            return None;
        }
        let s = cfstring_to_string(value);
        CFRelease(value);
        s
    }

    /// A raw AX pointer we deliberately move to the observer thread. Safe because
    /// CF/AX types are internally thread-safe and we retain it before sending.
    struct SendPtr(AXUIElementRef);
    unsafe impl Send for SendPtr {}

    /// Right after paste, snapshot the focused field, then check it once after a
    /// short delay for a one-word correction to learn.
    pub fn watch_correction(app: tauri::AppHandle, inserted: &str) {
        // Reading the focused field requires Accessibility permission.
        if !crate::paste::is_trusted() {
            return;
        }
        let inserted = inserted.to_string();
        let focused = unsafe { copy_focused_element() };
        if focused.is_null() {
            return;
        }
        let holder = SendPtr(focused);
        std::thread::spawn(move || {
            // Force whole-struct capture (2021 disjoint captures would otherwise grab
            // the raw pointer field and lose the `Send` impl on `SendPtr`).
            let holder = holder;
            std::thread::sleep(OBSERVE_DELAY);
            let after = unsafe { element_value(holder.0) };
            unsafe { CFRelease(holder.0) };
            let Some(after) = after else { return };
            let _ = app.emit_to("main", "openflow://autolearn/correction", serde_json::json!({"inserted":inserted,"after":after}));
        });
    }
}

#[cfg(target_os = "macos")]
pub use imp::watch_correction;

#[cfg(not(target_os = "macos"))]

#[cfg(not(target_os = "macos"))]
pub fn watch_correction(_app: tauri::AppHandle, _inserted: &str) {}

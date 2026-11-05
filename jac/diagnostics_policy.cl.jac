"""Portable diagnostic copy and platform wording."""

cl def:pub diagnostic_for(code: str, platform: str) -> dict {
    if code == "accessibility_not_granted" {
        if platform == "macos" {
            return {"headline": "Accessibility permission needed", "detail": "OpenFlow can't type into other apps yet. Open System Settings → Privacy & Security → Accessibility, turn on OpenFlow, then try again — no relaunch needed."};
        }
        if platform == "windows" {
            return {"headline": "Microphone access blocked", "detail": "Windows is blocking microphone access. Open Settings → Privacy & security → Microphone and turn on “Let desktop apps access your microphone”, then try again."};
        }
        return {"headline": "Microphone access blocked", "detail": "Linux could not access the microphone. Check the desktop privacy controls and confirm OpenFlow can use the selected input device."};
    }
    if code == "hotkey_tap_failed" {
        if platform == "macos" {
            return {"headline": "Fn key isn't wired up", "detail": "Accessibility shows granted, but the key listener failed to start — usually a stale permission entry left by an earlier build. Open System Settings → Privacy & Security → Accessibility, turn OpenFlow off and back on (or remove it with ‘−’ and re-add it), then relaunch OpenFlow."};
        }
        if platform == "windows" {
            return {"headline": "Right Ctrl isn't wired up", "detail": "The keyboard hook failed to install. Relaunch OpenFlow; if it keeps happening, another app may be holding a global keyboard hook (some anti-cheat or security tools do) — close it and relaunch."};
        }
        return {"headline": "Global key access is unavailable", "detail": "OpenFlow could not start its keyboard listener. Add your user to the input group, then sign out and back in before trying again."};
    }
    if code == "clipboard_unavailable" {
        return {"headline": "Couldn't paste", "detail": "The system clipboard couldn't be read or written, so the transcript wasn't inserted. Try again — if it persists, another app may be holding the clipboard."};
    }
    if code == "no_audio_captured" {
        if platform == "macos" {
            return {"headline": "No audio came in", "detail": "The microphone appears silent. Check System Settings → Privacy & Security → Microphone is on for OpenFlow, and that the right input device is selected."};
        }
        if platform == "windows" {
            return {"headline": "No audio came in", "detail": "The microphone appears silent. Check Settings → Privacy & security → Microphone allows desktop apps, and that the right input device is selected."};
        }
        return {"headline": "No audio came in", "detail": "The microphone appears silent. Check your desktop privacy controls and confirm the right input device is selected."};
    }
    if code == "empty_transcript" {
        return {"headline": "Didn't catch that", "detail": "OpenFlow heard audio but transcribed no words — try speaking a bit louder or closer to the mic."};
    }
    if code == "asr_unavailable" {
        return {"headline": "Speech model not installed", "detail": "OpenFlow needs its speech model (148 MB) to turn your voice into text. Click Download speech model — it installs in about a minute, with no relaunch."};
    }
    if code == "transcription_failed" {
        return {"headline": "Couldn't transcribe that", "detail": "The speech model hit an error on that recording. Try again — if it keeps happening, quit and reopen OpenFlow."};
    }
    return {"headline": "Something went wrong", "detail": "OpenFlow couldn't finish that action. Try again."};
}

# OpenFlow — Build Status

> Historical implementation notes below predate the Jac frontend migration.
> Current build and architecture status is in [JAC_MIGRATION_TRACKER.md](JAC_MIGRATION_TRACKER.md).

_Updated 2026-07-17. Tracks what's implemented vs. planned. Plan: `~/.claude/plans/make-a-new-project-abundant-stroustrup.md`; specs: `SPEC.md`, `ARCHITECTURE-DUAL-PLATFORM.md`._

## Done and verified

**Toolchain** — Rust 1.97.1 (+ aarch64/x86_64-apple-darwin targets), cmake 4.4, ninja, Node/pnpm, Tauri CLI 2.11. Xcode CLT + Developer ID present.

**Workspace** — Cargo workspace: `openflow-core`, audio/ASR/cleanup adapters, the local LLM worker, the standalone macOS Fn-key diagnostic, and `src-tauri`. The product client and meeting/MCP policy live in Jac under `jac/`; `ui/` retains only the Tauri CLI package.

- **`openflow-core`** — native-facing settings, session/statistics DTOs, and storage serialization. Jac owns dictation state transitions, cleanup policy, statistics shaping, dictionary behavior, and UI state.
- **`openflow-audio`** — cpal mic capture (mono f32 buffer + throttled RMS bars to the pill) + linear resample to 16 kHz. Wired into the shell: hold-Fn streams a live voice-reactive waveform and buffers the utterance.
- **`openflow-asr`** — whisper.cpp via whisper-rs on the **Metal GPU**, implementing `AsrEngine`. Verified: transcribes the standard test clip word-for-word. `ggml-base.en` (147 MB) auto-loaded from `~/Library/Application Support/OpenFlow/models/`. On finalize the shell resamples → transcribes → logs `[openflow] TRANSCRIPT: "..."` and drives the pill (recording → transcribing = real ASR time → done → idle). Parakeet/FluidAudio is the planned latency upgrade behind the same trait.
- **UI (`ui/`)** — design tokens ("Deep-Slate / Aqua-Open": slate pill, cyan/teal `#22C3B6`, Inter/Fraunces/JetBrains Mono); overlay **Flow Bar** (idle nub ↔ recording pill morph, RMS waveform canvas, accent dot) driven by `openflow://flowbar/state`; **Hub** shell (sidebar, deferred items greyed). tsc strict + vite build clean.
- **`src-tauri`** — accessory (menu-bar) app: tray with menu, transparent always-on-top **overlay pill** window (bottom-center anchored), hidden **Hub** window, event emit pipeline. App **launches and runs without crashing** (smoke-tested). Icons generated. Entitlements + capabilities in place.

## Core dictation loop — WORKING on macOS (validated)
Hold Fn → mic → whisper (Metal) → cleanup (per settings) → gates → clipboard paste into the
frontmost app (clipboard saved/restored). Each stage validated independently.
- **M3 insertion** — `paste.rs`: clipboard save/set + synthesized Cmd+V (CGEvent) + restore;
  Accessibility (`AXIsProcessTrusted`) checked at startup and before paste. (AX-direct / terminal
  chunked rungs from the plan layer on later.)
- **M5 cloud cleanup** — `openflow-cleanup`: **OpenAiProvider** (default) + **AnthropicProvider**,
  both sending the shared system prompt; keys read from the OS **keychain** (never a file);
  self-tested: cleans fillers, resolves self-corrections, adds punctuation.
- **M8 dictionary** — `openflow-core::dictionary`: store + edit-distance **prefilter** (incl. bigrams
  for split words), fed into the cleanup context. Manual add API + persistence. (4 tests.)
- **Settings** — `openflow-core::settings`: `CleanupMode` (Raw/Local/OpenAI/Anthropic) + level +
  models, persisted JSON; Tauri commands `get_settings`/`set_settings`/`get_status`.
- **M9 Settings UI** — Hub with a working **Cleanup Engine** pane (mode toggle), **Auto Cleanup**
  level cards, sound toggle, and a Home status view (Accessibility / key presence). Verified in-browser.

## Not yet built (remaining)
- **M4 local LLM** — Qwen2.5-1.5B GGUF is downloaded; the `CleanupMode::Local` path is stubbed
  (pastes raw). Needs a **separate worker process** to run llama.cpp (avoids a ggml symbol clash
  with the in-process whisper.cpp) — matches the plan's sidecar architecture. Eval harness pending.
- **M1 sidecar isolation** — the Fn hook + injection currently run **in-process** (works on macOS);
  moving them to the shared Rust sidecar process (per plan) is pending.
- **M0 remainder** — permissions onboarding UI, CI runner, notarize/signing pipelines.
- **M6 Windows (partial)** — the workspace now builds and links on Windows 11 (MSVC): `WH_KEYBOARD_LL`
  push-to-talk (Right Ctrl), clipboard+`SendInput` paste, foreground-process detection, Whisper ASR
  (CPU), and the tray/overlay/Hub shell all verified running end-to-end. Also added: an
  OpenAI-compatible `base_url` on the cloud cleanup provider so Windows users without an
  OpenAI/Anthropic key can point it at OpenRouter or similar. Remaining: GPU backend for Whisper/
  llama.cpp (CPU-only today), UIA-based insertion beyond clipboard paste, secure-input/elevated-window
  guards, a configurable hotkey (hardcoded to Right Ctrl), auto-learn (still macOS-only), and
  NSIS/signing for a real installer.
- **Dictionary auto-learn** (AX diff), history persistence + Hub history list, command mode.

## How to run what exists
```
cd ~/OpenFlow
cargo check --workspace
cargo check --tests --manifest-path src-tauri/Cargo.toml

# RUN THE APP (this is the one command — it shows the UI):
./dev.sh                                     # builds Jac assets, then starts the native app
```
**IMPORTANT — do NOT run `cargo build && ./target/...openflow-tauri` directly.** A plain
`cargo build` does not bundle the UI into the binary, so the window shows up blank. Use
`./dev.sh` for development. Tauri rebuilds the Jac client into static assets; no
frontend server runs. For a standalone `.app`, use `tauri build` to embed the same
Jac bundle.

A tray icon appears + a small dark "● OpenFlow" pill at the bottom-center of the screen.

```
# Standalone Fn-key detector (prints Fn DOWN/UP; auto-passes after 3 presses):
cargo run -p openflow-sidecar
```

**Hold-Fn demo test:**
- Press & HOLD Fn/Globe → pill morphs to the recording state (cyan dot + animated waveform).
- Release → brief "Cleaning up…" → "Done" flash → back to idle. (No audio/ASR yet — the
  pipeline completion is simulated so the full loop is visible.)
- Double-tap Fn quickly → hands-free lock (pill persists, shows "hands-free"); Fn again ends it.
- Quit: tray → Quit, or Ctrl-C.
- The Fn hook is listen-only for now, so Fn may also trigger your Mac's Globe action. To avoid
  that: System Settings → Keyboard → "Press 🌐 key to" → Do Nothing.
- If the pill never reacts: grant Input Monitoring to the terminal/app (System Settings →
  Privacy & Security → Input Monitoring), then relaunch.

# OpenFlow Jac migration

This folder holds the Jac source for OpenFlow's product UI and portable
application logic. The Jac client build emits static React assets under
`.jac/client/dist`; Tauri embeds and serves those files through its native host.
The installed app does not depend on a developer server.

## Current work

- `main.jac` routes native window labels to Jac client surfaces: the Hub window
  shows a Jac navigation shell with Home, Insights, Meetings, Dictionary, Notes,
  Settings, and Help; the meeting window shows `MeetingWindow`, and the floating
  window shows `Overlay`.
- `home.cl.jac` ports the dictation landing page, local history refresh/search,
  and local-first shortcuts; `styles.css` contains the Jac Hub shell and Home
  styling. Dashboard stats aggregation and recent history are in Jac; Tauri
  exposes local session records and clock/timezone inputs as data adapters.
- `insights.cl.jac` ports the local usage dashboard and activity charts. Its
  calculations share `stats.cl.jac` with Home and consume only local session
  rows and clock/timezone data from Tauri.
- `notes.cl.jac` ports the local scratchpad view with debounced local saving;
  `help.cl.jac` ports the in-app help page. Their local file access remains a
  narrow Tauri adapter.
- `settings.cl.jac` ports the primary settings controls for transcription,
  cleanup, shortcuts, keychain-backed credentials, and OS permissions. Secure
  key storage, permission prompts, and publik provisioning remain native service
  calls; Jac owns the settings view and interaction policy.
- `cleanup_policy.cl.jac` owns cleanup-level policy, prompt assembly, formatting
  guidance, few-shot examples, and deterministic over-editing gates.
  `cleanup_bridge.cl.jac` receives native ASR results, asks the native provider
  adapter to execute Jac-assembled messages, and returns a Jac-gated result.
  Native code owns provider credentials, HTTP transport, model execution, and
  billing effects; formatting and response policy remain in Jac.
- `dictation_policy.cl.jac` owns the portable hold, tap, hands-free, cooldown,
  cancel, session-cap, and pipeline-result transitions. `dictation_bridge.cl.jac`
  receives native event-tap inputs and sends transition actions back to Tauri.
  macOS hotkey capture, microphone, ASR, paste, and overlay effects stay native.
  There is no Rust shadow reducer; live parity and non-macOS runtime verification
  remain open.
- `meeting_policy.cl.jac` owns transcript formatting instructions and short-note
  versus generated/unavailable summary policy. `meeting_workflow.cl.jac` owns
  meeting input validation, local summary generation, transcript acceptance,
  record shaping, and save orchestration. Rust returns the final local Whisper
  transcript and persists the opaque JSON record; audio capture, inference, and
  atomic file access remain native effects.
- `autolearn_policy.cl.jac` owns the conservative one-word name-correction
  detector. The native macOS Accessibility adapter snapshots the focused field
  after paste and sends both strings to `autolearn_bridge.cl.jac`; Jac decides
  whether to add an auto-learned dictionary entry through the local storage
  command.
- `overlay.cl.jac` owns overlay state, native event subscriptions, actions, and
  presentation; `overlay.css` holds its styling.
- `meeting.cl.jac` ports the live note window and compact meeting reminder,
  including transcript, thoughts, summary, and native meeting commands. Its
  existing visual stylesheet is bundled from `meeting.css`.
- `meetings_hub.cl.jac` ports the meetings library, local transcript import,
  calendar accounts/events, and recording controls. Its styles are in
  `meetings-hub.css`.
- `openflow/core/dictionary.jac` is the first portable product-logic port.
- `meetings_store.jac` and `meetings_mcp.jac` own local archive queries and the
  stdio MCP protocol. The standalone entry point is
  `scripts/meetings-mcp.sh`; only the optional local inference worker remains a
  native binary adapter.
- `jac.toml` includes the Tauri JavaScript API for client components.

The Jac Hub shell, all Hub pages, meeting note window, overlay, and cleanup
request/response bridge compile successfully. Tauri now builds and embeds this
Jac client bundle for development and packaging. Each native webview loads the
same `index.html`; Jac selects the Hub, overlay, or meeting surface from the
native window label. The app has been launched and the Home and Meetings views
visually checked from this static Jac bundle. A locally signed Tauri package
was installed, deep signature verification passed, and the Home view was
visually checked after launch. It is signed with an Apple Development identity
and is not notarized; release signing still needs Developer ID credentials.

The React/TypeScript UI source and Vite build setup have been removed. Jac is the
configured frontend, while local persistence and OS integration still go
through narrow Tauri commands.

## Commands

From this folder:

```sh
jac check main.jac cleanup_bridge.cl.jac cleanup_policy.cl.jac dictation_bridge.cl.jac dictation_policy.cl.jac meeting_workflow.cl.jac meeting_policy.cl.jac autolearn_bridge.cl.jac autolearn_policy.cl.jac home.cl.jac insights.cl.jac dictionary.cl.jac notes.cl.jac settings.cl.jac help.cl.jac overlay.cl.jac meeting.cl.jac meetings_hub.cl.jac stats.cl.jac
jac check openflow/core/dictionary.jac meetings_store.jac meetings_mcp.jac
jac build main.jac --client web
```

Portable product behavior in Rust remains only where migration slices are not
yet complete or where code performs native OS, audio, inference, secure
credential, network, or local file effects.

The migration target is all portable product logic and UI in Jac. Keep only
platform adapters for Tauri/window lifecycle, macOS permissions and keychain,
audio capture, global hotkeys, text insertion, native inference engines, and
signing/packaging. Rust remains during the transition until each Jac slice has
behavioral parity and the shipped package starts offline without a server.

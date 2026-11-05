# OpenFlow Jac migration tracker

This tracker compares the current app to each migration milestone. It records
source size, build/iteration time, and runtime latency so the migration can show
whether it saves code or developer time without slowing dictation.

## Baseline captured 2026-11-04

Source count is nonblank physical lines in `crates/`, `src-tauri/src/`,
`src-tauri/macos/`, and `ui/src/`. Generated files and scripts are excluded.

| Language | Baseline lines |
|---|---:|
| Rust | 11,721 |
| Swift | 142 |
| TypeScript and TSX | 4,300 |
| Jac | 0 |
| **Total** | **16,163** |

The snapshot includes the checkout's existing uncommitted implementation as it
stood before this migration. Lines saved are measured as baseline total minus
current product-source total; negative values mean the migration has added
source so far.

## Build and iteration baseline

| Measurement | Baseline |
|---|---:|
| UI production build, first sample | 2.67 s |
| UI production build, warm median (3 runs) | 1.969 s |
| `cargo check --workspace`, initial build | 30.80 s |
| `cargo check --workspace`, warm median (3 runs) | 0.331 s |
| Jac client build probe, warm median (3 runs) | 3.208 s |
| Jac Hub + meeting + overlay bundle, warm median (3 runs) | 3.714 s |
| Jac shell + Home + Meetings + meeting note + overlay bundle, warm median (3 runs) | 3.832 s |
| Current Jac shell + Home + Meetings bundle, warm median (3 runs) | 3.970 s |
| Jac Home with Jac stats aggregation + shell/meeting surfaces, warm median (3 runs) | 3.732 s |
| Jac shell + Home + Dictionary + Meetings bundle, warm median (3 runs) | 3.625 s |
| Jac Hub + all pages + cleanup bridge bundle, warm median (3 runs) | 4.890 s |
| Jac MCP + archive static-check loop, warm median (3 runs) | 1.605 s |
| Jac client with dictation and meeting-summary bridges, warm median (3 runs) | 4.711 s |
| Jac client with dictation, meeting-summary, and auto-learn bridges, warm median (3 runs) | 4.603 s |
| Jac client after auto-learn Unicode tokenization update, warm median (3 runs) | 4.614 s |
| Jac client after Jac cleanup layout/provider fallback, warm median (3 runs) | 5.712 s |
| Jac client after Jac cleanup output-token budget, warm median (3 runs) | 4.944 s |
| Jac client after permission onboarding gate, warm median (3 runs) | 4.777 s |
| Jac client after Hub error/publik notices and first-run card, warm median (3 runs) | 4.586 s |
| Jac client after shared inline speech-model banner, warm median (3 runs) | 4.597 s |
| Jac client after model-state event bridge correction, warm median (3 runs) | 4.582 s |
| Current Tauri app `cargo check`, warm median (3 runs) | 0.206 s |
| macOS Tauri check with shared cleanup bridge, warm median (3 runs) | 0.177 s |
| macOS Tauri check after shared cleanup adapter consolidation, warm median (3 runs) | 0.180 s |
| macOS Tauri check after Jac cleanup output-token budget, warm median (3 runs) | 0.215 s |
| Jac client build / signed Tauri package | Jac bundle is now integrated; signed package verification pending |
| Jac edit-to-running UI loop | Tauri dev now builds static Jac assets; repeatable loop timing pending |

The UI and Rust measurements were taken on the same Mac and checkout. The Rust
initial build compiled native dependencies; it is not comparable to the warm
median. The first Jac measurement was a small branded build probe. The 3.714
second measurement builds the Meetings Hub, overlay, and meeting window
components, but still does not include the other Hub pages. This is not a
feature-equivalent comparison to the complete React app. Build numbers are
development feedback, not user-facing runtime speed.

## Runtime baseline

Dictation and meeting latency are **not measured yet**. Use the same local speech
model, fixed consented audio clips, Mac, and settings for baseline and migrated
runs. Track ASR duration, end-to-end time from key release to inserted text,
and meeting live-transcript lag as separate metrics. Do not use live meeting
audio as an unannounced benchmark fixture.

## Ongoing measures

- `source_snapshot`: per-language nonblank lines and lines saved vs 16,163.
- `dev_iteration`: per-run duration of a build or repeatable edit-to-running
  command; `dev_iteration_median`: median for the requested runs.
- Runtime rows will be added when the migration includes a repeatable local
  benchmark fixture. Each comparison must retain the model/device/clip details.

## Migration inventory

| Slice | Current state | Remaining work |
|---|---|---|
| Hub, overlay, meeting windows | Jac client is the configured Tauri frontend; Home and Meetings were rendered during prior verification. The permission gate, persistent skip, error/publik banners, full publik account controls in Settings, first-run publik card, inline speech-model banner, and the three coming-soon navigation pages are Jac-owned. Model download state and UI are connected through a Jac-to-Jac browser event. | Continue visual/behavior verification of the Jac frontend and runtime bridge |
| Dictation cleanup | All three platform paths send raw transcript, settings facts, and stored vocabulary through a shared Jac event bridge. Jac selects relevant terms, owns provider fallback, prompt assembly, formatting examples, layout-cue normalization, output-token budgeting, OpenAI/Anthropic request payload construction and response interpretation, cloud ASR response interpretation, output post-processing, and the deterministic acceptance gate. Rust supplies stored vocabulary, settings facts, and app facts; performs HTTP transport/local inference; and preserves Publik billing effects. Duplicate Rust prompt, payload builder, response decoders, layout normalizer/post-processor, matcher/gate, token-budget helper, and legacy provider trait/default-budget routes have been removed. | Verify Windows/Linux builds and live bridge behavior; keep Rust limited to transport, credentials, local inference, and billing effects |
| Dictation state/orchestration | Jac is the sole PTT/tap/lock/session-cap/finalization reducer. All platform adapters send JSON facts/actions to Jac; there is no legacy Rust event-state reducer, Rust action model, or sidecar wire protocol. Native adapters consume Jac action facts directly and perform OS/audio/paste effects. Jac also shapes completed records, applies the sound-on-start preference, and decides whether captured dictation audio clears the speech floor and is long enough to count as an intentional attempt. Native adapters retain key listeners, audio measurement/capture, ASR, paste, sound playback, and local stats persistence. | Verify live hold/tap/hands-free/cancel/finalize, audio-policy response, and record persistence on each OS |
| Meeting notes and calendar | Jac owns note UI, archive search and ordering, live rolling-transcript overlap/deduplication, transcript validation/formatting/summary policy, record shaping and save orchestration, standalone stdio MCP protocol, calendar polling cadence, query window, provider-event normalization, upcoming-event filtering/sorting, and reminder deduplication. Rust owns recording/capture, final ASR, OAuth, Calendar API requests, opaque local JSON file access, and window effects. | Verify saved-note compatibility with the MCP archive reader and test the meeting workflow on other supported OSes; retain device capture, OS auth, local file access, and inference adapters |
| Dictionary | Jac owns the entry DTO, relevance matching, case-insensitive merge/de-duplication, auto/manual source policy, and removal. Rust only reads/writes the local JSON file and preserves the legacy `source` field for compatibility; the Rust `DictionaryStore` domain model is removed. | Verify manual edits and auto-learn against a live local dictionary; keep only file access and backward-compatible serialization in the native adapter |
| Settings and credentials | Jac settings UI owns the cleanup/ASR mode and level vocabulary, validates updates, selects product defaults, and migrates absent legacy fields. Rust carries a typed DTO and reports which fields were missing; it retains file persistence and the early Linux push-to-talk bootstrap needed before Jac mounts. Keychain and OS prompts remain native. | Verify upgrade/default migration in a rebuilt installed app; retain credential and permission effects |
| Diagnostics, auto-learn, model lifecycle | Jac owns diagnostic code interpretation, copy/platform wording/error timing, model candidate order and pinned download spec, the model lifecycle reducer, prompt/retry/progress/failure copy, and unavailable-state decisions. Rust emits code/platform facts and retains native UI event effects plus last-error state; the Rust failure enum is removed. Rust also retains model scanning/loading, streaming/hash verification, and the duplicate-download guard; macOS Accessibility snapshots and local dictionary persistence remain native adapters. | Verify Linux-specific diagnostic copy and live diagnostics/model download plus an actual learned correction; keep only native error presentation/storage and filesystem, network streaming, hashing, and inference effects in adapters |
| Windows/Linux paths | Cleanup, dictation transitions, and dictation record shaping now route through Jac. Hotkey listening, capture, ASR, local settings/dictionary/stats persistence, model access, and paste remain in native adapters. | Move remaining portable settings/provider and persistence policy to Jac; verify supported targets |
| Signed installed app | Local Apple Development-signed package now embeds Jac and launches | Developer ID signing/notarization and cleanup-bridge session verification remain |

The installed `/Users/vatsalshah/Applications/OpenFlow.app` currently has a
valid Apple Development signature, but the local keychain currently exposes no
Developer ID Application identity. The configured release script requires
Developer ID and refuses an Apple Development release. This leaves notarized
distribution pending; it does not block local Jac development.

Latest source snapshot is recorded in `docs/jac-migration-metrics.csv`; use the
latest `source_snapshot` row for current nonblank lines and savings. After the
React source removal, cumulative source size is now below the original baseline.
The target remains all portable product behavior and UI in Jac, while native
macOS/Tauri, audio, OS, keychain, text insertion, and inference adapters remain
native. Tauri now embeds `jac/.jac/client/dist`; Hub, overlay, and meeting
windows load the same Jac entry point and dispatch by native window label. A
local Tauri dev launch visually rendered Jac Home and Meetings screens with no
frontend server. A local package was built, deep signature verification
passed, it was installed, and its Home screen rendered. It uses Apple
Development signing without notarization; Developer ID distribution remains
pending.
Home and Insights share the Jac stats aggregator, including lifetime/today
WPM, best WPM, streak, daily activity, and estimated typing time saved; Tauri
supplies Jac-shaped local session records as JSON plus clock/timezone inputs.
The native stats adapter owns only the `sessions` file envelope and local I/O.
Settings UI and
interaction policy are now in Jac; the publik account card also has wallet
refresh, plan/usage links, reconnect, own-key selection, cost details, and a
forget-key confirmation. Native commands retain keychain writes, permission
prompts, allow-listed external links, and publik provisioning. Cleanup prompt assembly,
formatting examples, and deterministic gates are in Jac. A Jac event bridge now
submits Jac-built messages to native provider/model adapters and returns the
gated transcript. Tauri now loads the Jac runtime in all windows, so the bridge
can register in the live Hub. The installed Jac app is running; record a
consented dictation session before claiming end-to-end cleanup bridge
verification. The
Jac Hub + all pages + cleanup bridge bundle built in a warm median of 4.890 s,
compared with the 1.969 s React UI build baseline. These are build times, not
runtime speed measurements. No Developer ID signed or notarized Jac package
has been produced.
The dictation bridge and reducer compile into the Jac bundle. Windows and Linux
hotkey events, hands-free toggles, overlay controls, periodic ticks, and native
pipeline completion now use the Jac reducer; native adapters still perform
microphone capture, transcription, and text insertion. On this Mac, the Jac
client build and macOS Tauri crate check pass. A Linux cross-target check stops
before project compilation because pkg-config has no Linux sysroot. The Windows
Rust target is installed, but its check stops in the `ring` build script because
the MinGW C compiler is absent. Neither platform branch is type-checked yet. No
live cross-platform dictation was exercised. At the prior dictation-state
snapshot, source was 14,117 lines (Rust 11,050,
Swift 142, TypeScript 0, Jac 2,925), 2,046 lines below baseline. The Jac
client build median was 5.370 s and the macOS Tauri check median was 0.171 s
across three runs. These are developer build timings; no runtime speedup is
claimed. The Jac MCP validator endpoint
returned a runtime setup error during an earlier dictation slice, so the pinned
local Jac client build and Tauri cargo check were used as compile checks.
The standalone meeting MCP protocol and archive search now run from Jac through
`scripts/meetings-mcp.sh`; the duplicate Rust JSON-RPC server was removed. Jac
static checks and Tauri cargo check pass. Three Jac checker command runs took a
1.605 s median including command startup (0.94 s inside the checker). Its first
runtime MCP session has not yet been exercised. The shell requires the pinned
Jac runtime on the MCP host; bundled runtime support remains open. Meeting
formatting and automatic-summary prompt/gate policy now runs in Jac and calls
the existing local model adapter through an event bridge. The bridge compiles,
but a completed recording and summary have not yet been verified end to end. The
full Jac client build with both dictation and meeting bridges has a 4.711 s warm
median across three runs; no dictation or meeting runtime-speed increase has
been established.
Auto-learn detection now runs in Jac after the native Accessibility observer
emits a before/after field snapshot. The Jac bundle and Rust adapter compile,
but an actual post-paste correction has not yet been observed and learned. The
full client build after auto-learn integration measured 4.603 s median over
three runs; these build timings do not establish transcription or interaction speed.
Savings will appear when a slice is integrated and its previous implementation
is removed.

Diagnostic identifiers are emitted by native dictation/model adapters and
resolved to copy in `jac/diagnostics_policy.cl.jac`. Jac controls the 4.5 s
error-state linger and calls Tauri only for window/event effects and last-error
storage. Runtime-provided diagnostic text follows the same bridge. The Jac
client build and Tauri cargo check pass; three client-build runs measured a
4.740 s median including command startup. No interactive diagnostic flow has
been exercised after this migration.

The missing-model prompt, retry gate, progress calculation, successful install
state, candidate ordering, pinned download specification, failure copy, and
unavailable-state response are now in `jac/model_bridge.cl.jac` and
`jac/model_policy.cl.jac`. Jac owns lifecycle reduction and UI state; the native
adapter emits lifecycle inputs and retains model scanning/loading,
download/hash verification, and inference effects. The bridge subscribes before
initialization, so it does not need a separate native status snapshot. The old
React prompt is no longer the configured frontend, though its source remains
until legacy UI removal. Runtime download and visual app verification remain
open. The client bundle and Tauri crate compile after this slice; new build
timing and source counts are in the metrics CSV.

All three platform cleanup paths send raw transcription and settings facts
through a shared Tauri event adapter to Jac. Jac selects provider fallback,
builds prompts, chooses output-token budgets, normalizes spoken line breaks,
post-processes model output, and applies the acceptance gate. The OpenAI,
Anthropic, Publik, and local inference adapters accept Jac's budget as input.
Windows/Linux provider adapters are wired to the Jac command, while audio
capture and text insertion remain platform code. The macOS Jac bundle and
Tauri crate compile; the Linux cross-target check stopped in dependency setup
because pkg-config lacks a Linux sysroot, and Windows compilation is not
available here. No cleanup runtime flow has been exercised after this routing
change. The final Jac client median is 4.944 s and the macOS Tauri check median
is 0.215 s. The app-local Rust local-inference wrapper that chose its own token
budget has been removed; compatibility helpers remain in `openflow-cleanup` for
legacy callers and should be retired after runtime parity is established. The
latest source snapshot is 18,395 lines (Rust 11,163, Swift 142, TypeScript
4,300, Jac 2,790), or 2,232 lines above the pre-migration baseline. After
moving dictionary matching and editing policy to Jac, the full Jac client build
has a 4.926 s median across three runs; the earlier matching-only slice measured
5.691 s. These are development build times, not runtime performance. The Tauri
crate check after replacing native dictionary mutations with a save adapter has
a 0.181 s median across three runs. Runtime matching and dictation latency have
not been benchmarked. See the `jac-client-dictionary-crud-policy`,
`rust-check-dictionary-storage-adapter`, and latest `source_snapshot` rows.
The earlier `jac-client-dictionary-prefilter-final` and
`rust-check-jac-dictionary-prefilter` rows retain the staged measurements. The
prior settings slice is recorded as `jac-client-settings-publik-account-final`,
and the Rust gate removal as `rust-check-jac-owned-cleanup-gate-removal`.
Earlier comparisons remain in the CSV, including `jac-client-cleanup-budget-final`, `rust-check-cleanup-budget-verified`,
`jac-client-onboarding-gate-final`, `jac-client-hub-notices-first-run`,
`jac-client-model-banner-bridge`, and latest `source_snapshot` rows in the
metrics CSV.

The rolling live-transcript overlap/deduplication policy has moved from the
native meeting recorder to `jac/meeting_policy.cl.jac`; the native capture
worker now emits each local Whisper rolling window and the Jac note window
combines it. Calendar event selection, recurring sync cadence, reminder
deduplication, and reminder dispatch have since moved to a Jac bridge; Rust
retains OAuth, Calendar API requests, local cache access, and native window
display. The Jac client bundle builds, and the Tauri crate check passes. Three
Jac client builds for the rolling-transcript slice measured a 6.116 s warm
median; three Tauri crate checks measured a 0.568 s median. The calendar Jac
client builds in a 4.978 s median, and the Tauri crate check takes 0.178 s,
each across three runs including command startup. The latest source snapshot is
18,450 lines (Rust 11,116, Swift 142, TypeScript 4,300, Jac 2,892), or 2,287
lines above baseline at that snapshot. Neither a live recording nor live
calendar reminder was exercised, so runtime behavior and latency remain
unverified.

The React/TypeScript UI source and Vite build setup have now been removed after
confirming Tauri's `frontendDist` points to the Jac client bundle and translating
the remaining placeholder navigation pages. `ui/` now retains only the Tauri
CLI package. At that snapshot, source was 14,160 lines (Rust 11,113, Swift
142, TypeScript 0, Jac 2,905), which is 2,003 lines below the pre-migration
baseline. The Jac
client build measured a 5.510 s median across three runs including command
startup; the Tauri crate check measured a 0.264 s median across three runs. The
source-size savings are real; no runtime speed gain is claimed.

Dashboard aggregation and recent-history selection have now been removed from
Rust and centralized in `jac/stats.cl.jac`. Rust persists complete session
records and exposes those records plus clock/timezone inputs; Jac calculates
the metrics and builds recent history. The latest snapshot is 13,946 lines
(Rust 10,881, Swift 142, TypeScript 0, Jac 2,923), or 2,217 lines below the
baseline. The Jac client build median is 5.048 s and the Tauri crate check
median is 0.179 s, across three runs each. These are development timings; live
dashboard rendering was not rechecked in this slice.

At the record-policy checkpoint, completed-dictation record shaping (whitespace word count, character count, and
duration conversion) now runs in `jac/stats.cl.jac` through a request/resolve
bridge. Rust retains the timestamp input and persists the Jac-shaped record to
the local stats file; a native fallback remains for bridge startup/timeout
recovery. That source snapshot was 14,239 lines (Rust 11,127, Swift 142,
TypeScript 0, Jac 2,970), 1,924 lines below baseline. Across three runs the Jac
client build median was 5.076 s and the macOS Tauri check median was 0.177 s.
The bridge compiles but has not been exercised by a live dictation, so record
parity and runtime performance are not yet confirmed.

The duplicate Rust dictation reducer and its timing constants have now been
removed from `openflow-core`. The macOS event tap, Windows keyboard hook, and Linux
keyboard listener only transport inputs to Jac; pending macOS input queues until
the Hub bridge registers. Jac alone decides state transitions, and native
adapters execute capture, ASR, paste, sound, and overlay actions. Jac and macOS
crate builds pass. Windows cross-compilation stops in the `ring` build script
because MinGW is missing; Linux cross-compilation stops in the X11 build script
because no pkg-config sysroot is configured. That reducer-removal snapshot was
13,820 lines (Rust 10,708, Swift 142, TypeScript 0, Jac 2,970), or 2,343 lines
below baseline. The Jac build median was 5.086 s and the macOS Tauri check
median was 0.169 s across three runs. No live dictation was run, so startup
queueing, reducer parity, and runtime latency remain unverified.

The sound-on-start choice now lives in the Jac reducer. The Jac bridge reads
the saved preference on mount and receives updates from the native settings
command; macOS keeps only the `afplay` effect. The Tauri command and Jac bundle
compile. The latest source snapshot is 13,822 lines (Rust 10,704, Swift 142,
TypeScript 0, Jac 2,976), 2,341 lines below baseline. The Jac client build
median is 5.225 s and the macOS Tauri check median is 0.171 s across three runs.
This setting path has not been exercised in a live recording.

Publik settings balance, last-charge, and weekly-usage labels now format in
`jac/publik_policy.cl.jac` from the raw micro-dollar values. The Rust status
adapter no longer constructs or emits those display strings; it retains the
wallet facts, secure key handling, network requests, and persisted state. The
Jac client build and macOS Tauri crate check pass. Three Jac client builds
measured a 5.088 s median and three crate checks a 0.174 s median. The latest
source snapshot is 13,823 lines (Rust 10,681, Swift 142, TypeScript 0, Jac
3,000), or 2,340 lines below the recorded baseline. These are development
iteration timings; dictation and meeting runtime performance have not been
measured. The Jac MCP validator service returned a runtime setup error, so the
pinned local Jac client build served as the compile check. No live Publik
wallet/settings flow was exercised.

Microphone permission guidance copy now lives in
`jac/permissions_policy.cl.jac`. The native permission watcher still samples OS
grant state while the Hub is asleep and sends only raw grant/charged-app facts;
Jac derives the onboarding sentence. Completed-dictation record shaping is now
Jac-only across the macOS, Windows, and Linux adapters: native `count_words` and
the duplicate record fallback were removed, and the bridge waits within its
three-second response budget for Jac registration before returning a record for
local persistence. The Jac build and macOS Tauri check pass; three-run medians
are 5.111 s and 0.171 s. The latest snapshot is 13,637 lines (Rust 10,440,
Swift 142, TypeScript 0, Jac 3,055), 2,526 lines below baseline. No live
permission or dictation stats interaction was exercised; runtime speed remains
unmeasured. The Jac MCP validator returned the same runtime setup error, so the
local client build is the available Jac compile check.

The remaining duplicate cleanup implementation in the Rust core/provider crate
has been retired. `openflow-core::cleanup` now exposes only the level/context and
message DTOs needed at the native boundary; the Rust prompt, vocabulary/context
assembly, layout normalization/post-processing, provider trait, and related
helpers are deleted. OpenAI, Anthropic, and Publik adapters require Jac's
explicit message list and output-token budget; no provider convenience method
can silently choose Rust cleanup policy. The example now uses the explicit
adapter contract. Workspace and provider-example checks pass. Existing Rust
cleanup behavior tests were removed with the migrated implementation; tests
were not run or added. Live provider parity remains unverified.

Overlay positioning has moved out of the shared Rust core and Tauri orchestration
into `jac/overlay_policy.cl.jac`. The Jac overlay now converts raw display/work
area measurements to logical points, centers and clamps its position, and asks
the native adapter to apply the result and show the window. Rust retains monitor
discovery, native bounds updates, and window movement only. The Jac client build
and macOS Tauri crate check pass; their three-run medians are 5.102 s and
0.169 s. At that overlay-placement checkpoint, source was 13,695 lines (Rust
10,516, Swift 142, TypeScript 0, Jac 3,037), 2,468 lines below baseline. No runtime interaction was exercised;
the already-installed OpenFlow process was left running and untouched. The
Jac MCP validator still fails during runtime setup, so local Jac bundle builds
remain the available compile verification.

The obsolete Rust cleanup path has now been removed: only Jac's explicit
messages and selected token budget can reach the OpenAI, Anthropic, or Publik
HTTP adapters. The provider interface no longer assembles prompts or chooses a
fallback budget; Rust prompt/layout helpers and the cleanup-level policy methods
are deleted. A three-run Jac client build median is 5.134 s; the workspace
cargo-check median is 0.191 s. The current snapshot is 12,833 lines (Rust 9,636,
Swift 142, TypeScript 0, Jac 3,055), 3,330 lines below the original baseline.
These are developer build measurements; runtime latency and live provider
parity remain unverified. The Windows and Linux branches have not been
type-checked because this Mac lacks their target system dependencies.

Dictation's audio speech-floor and short-attempt decisions now run in
`jac/audio_policy.cl.jac`; macOS and Windows adapters send measured peak, RMS,
and duration through a local Tauri event bridge. Capture and measurement stay
native. The Jac client build and macOS workspace check pass; the three-run
medians for this slice are 5.591 s and 0.203 s. The latest source snapshot is
12,941 lines (Rust 9,702, Swift 142, TypeScript 0, Jac 3,097), 3,222 lines
below baseline. This slice adds the bridge and therefore does not claim a
per-slice line reduction. The bridge has not been exercised in a live recording;
Windows remains uncompiled on this host. No dictation latency improvement is
claimed.

The Publik account's first-run card and plan-button labels/link selection now
come from `jac/publik_policy.cl.jac`; Rust returns raw account facts and keeps
the `publikhq.com` URL validation before opening a browser. The Jac client build
median is 5.201 s and `cargo check --tests` median is 0.190 s. The source
snapshot is 12,778 lines (Rust 9,496, Swift 142, TypeScript 0, Jac 3,140),
3,385 lines below baseline. The Rust test target type-checks, while linking the
test executable fails on this host with unresolved `___isPlatformVersionAtLeast`
from the bundled Metal Whisper objects. No live UI verification occurred: the
computer-use surface reports macOS locked. The installed app currently displays
`Can't find variable: summary_copy`; the source bundle now builds with that
meeting-view variable declared, but the installed app has not been relaunched.
Existing local records show dictation and meeting transcripts were saved in
prior sessions; one archived meeting has `summary_status=unavailable`. This is
historical evidence only, not a live pass.

Record snapshots and build timings with:

```sh
(cd jac && jac run ../scripts/migration_metrics.jac snapshot)
(cd jac && jac run ../scripts/migration_metrics.jac benchmark jac-client 3 --cwd jac -- jac build main.jac --client web)
(cd jac && jac run ../scripts/migration_metrics.jac benchmark rust-check 3 -- cargo check --manifest-path src-tauri/Cargo.toml)
```

Measurements append to `docs/jac-migration-metrics.csv`.

## Legacy IPC retirement — 2026-11-05

The unused Rust IPC crate and Rust input/event enums have now been removed.
Native hotkey and capture code passes small JSON facts directly to the Jac
dictation reducer, and Rust retains only effect/data-transfer types needed by
native integrations. The current source snapshot is 12,343 lines (Rust 9,061,
Swift 142, TypeScript 0, Jac 3,140), 3,820 lines below baseline. The three-run
`cargo check --workspace` median is 0.195 s. The workspace check and
`git diff --check` pass, and a symbol search finds no references to the retired
IPC crate or event enums. `cargo fmt --check` still reports formatting diffs in
Rust files; these have not been mass-reformatted as part of the migration. Test
targets type-check per the prior `cargo check --tests` run, but test executable
linking and live dictation/note-taking behavior remain unverified on this host.

Meeting summary JSON parsing and required-field validation have moved from a
Rust Tauri command into `jac/meeting_bridge.cl.jac`. Jac now owns response
decoding as well as prompt construction, fallbacks, transcript acceptance, and
summary-state selection; Rust keeps the local model/provider and event
transport effects. Jac client build and `cargo check --workspace` pass, and
`git diff --check` is clean. The Jac client build median is 5.166 s and the
workspace check median is 0.171 s across three runs. The source snapshot is
12,328 lines (Rust 9,046, Swift 142, TypeScript 0, Jac 3,140), 3,835 lines below
baseline. This verifies compilation only; live summary generation and the
dictation/note-taking flows still require an unlocked Mac for runtime checks.

Meeting-input preparation has moved to `jac/meeting_policy.cl.jac`: Jac now
trims and validates recognized transcript text and chooses the untitled-note
default before the native file adapter persists a meeting. The meeting bridge
returns that decision through a local Tauri event; Rust keeps the channel and
local JSON file effects. Jac client build and `cargo check --tests` pass, and
`git diff --check` is clean. The Jac build median is 5.225 s and Rust test-target
check median is 0.173 s over three runs. The current snapshot is 12,414 lines
(Rust 9,100, Swift 142, TypeScript 0, Jac 3,172), 3,749 below baseline. This
bridge adds code in this slice; it does not claim a per-slice line reduction.
Jac MCP validation is currently failing before source analysis because its
runtime reports `JacRuntime has no attribute setup`; the local Jac client build
passes. Live meeting save and dictation behavior remain unverified.

The low-balance Publik banner threshold, copy, and once-per-dip display state
now live in `jac/publik_policy.cl.jac` and the Jac Hub. Rust publishes the
starter grant and remaining balance as raw facts; it retains provider error
transport, wallet parsing, and the allow-listed URL-opening effect. The former
Rust threshold/banner builder and its duplicate native display latch were
removed. The Jac client build and Rust test-target type-check pass, as does
`git diff --check`; the obsolete Rust policy symbols are absent. The latest
source snapshot is 12,348 lines (Rust 8,991, Swift 142, TypeScript 0, Jac
3,215), 3,815 below baseline. Three-run medians: 5.791 s for the Jac client
build and 0.699 s for `cargo check --tests` (including command startup and an
incremental compile on the first run). Jac MCP validation still fails before
analyzing source with `JacRuntime has no attribute setup`; the local Jac build
passes. The locked Mac prevents live UI verification.

The Publik disclosure justification was also removed from the Rust status DTO;
Jac is now its only source. After this cleanup the current snapshot is 12,337
lines (Rust 8,980, Swift 142, TypeScript 0, Jac 3,215), 3,826 below baseline.
The latest three-run medians are 5.251 s for the Jac client build and 0.168 s
for Rust test-target type-checking. Both builds pass after the DTO cleanup.

The 402/exhausted Publik banner has now also moved to Jac. Rust sends the raw
gateway message and wallet/account facts; Jac selects the headline, suffix,
button label, and link. The native side retains the allow-listed system-browser
effect. The Rust `PublikNotice` presentation DTO and its notice builder/tests
were removed. Jac client build and Rust test-target type-check pass, and
`git diff --check` is clean. The current snapshot is 12,232 lines (Rust 8,871,
Swift 142, TypeScript 0, Jac 3,219), 3,931 below baseline. Three-run medians:
5.328 s for the Jac client build and 0.169 s for `cargo check --tests`.
These checks do not establish live banner behavior; the Mac is still locked.

## Publik provisioning error policy — 2026-11-05

Publik provisioning failures now cross the native boundary as structured error
facts. Rust retains transport/provider error classification, retry metadata,
HTTP status/body data, and Tauri error delivery; Jac formats the user-facing
Settings message, including retry timing and safe fallbacks. This removes the
remaining provisioning error-copy table and HTTP body message extraction from
Rust. The Jac client build and `cargo check --tests --manifest-path
src-tauri/Cargo.toml` pass; `git diff --check` is clean. Three-run medians are
5.929 s for the Jac client build and 0.210 s for the Rust test-target check.
The latest source snapshot is 12,272 lines (Rust 8,878, Swift 142,
TypeScript 0, Jac 3,252), 3,891 lines below the recorded baseline. This slice
adds the structured cross-layer contract and does not claim a per-slice LOC
reduction. No live provisioning UI was exercised; the locked Mac still prevents
interactive verification.

## Google Calendar event normalization — 2026-11-05

The native Calendar adapter now returns provider event payloads grouped by
account. Jac owns conversion into app meeting events: required ID/start/end
validation, dateTime versus all-day date selection, untitled-event fallback,
join-link normalization, and cancellation state. OAuth, token storage, HTTP
requests, provider response transport, and local event-cache writes remain in
the native adapter. Jac client build and `cargo check --tests --manifest-path
src-tauri/Cargo.toml` pass, and `git diff --check` is clean. Three-run medians
are 7.623 s for the Jac client build and 0.328 s for the Rust test-target
check. The current snapshot is 12,285 lines (Rust 8,856, Swift 142,
TypeScript 0, Jac 3,287), 3,878 below baseline. This adapter boundary adds 13
net source lines in the slice; no per-slice LOC reduction is claimed. Live
Google Calendar sync and reminder behavior have not been re-run in the UI.

## Calendar query-window policy — 2026-11-05

The seven-day Google Calendar query horizon now lives in
`jac/calendar_policy.cl.jac`. The Jac Calendar bridge computes UTC ISO query
bounds and passes them to the native API adapter; Rust no longer chooses the
calendar look-ahead duration or constructs its time window. OAuth, HTTP query
transport, and account token handling remain native. Jac client build,
`cargo check --tests --manifest-path src-tauri/Cargo.toml`, and `git diff
--check` pass. Three-run medians are 7.997 s for the Jac client build and
0.318 s for the Rust test-target check. The current snapshot remains 12,285
lines (Rust 8,846, Swift 142, TypeScript 0, Jac 3,297), 3,878 lines below
baseline; this slice moved 10 lines from Rust to Jac without changing the total.
Runtime dictation/meeting performance is still awaiting a repeatable local audio
fixture, as described in the Runtime baseline section.

## Settings form validation policy — 2026-11-05

The Jac Settings page now validates the cleanup/ASR modes, cleanup levels,
push-to-talk choices, and supported editable field names before sending the
settings DTO to Tauri. Jac normalizes endpoint trailing slashes and restores
empty model fields to their documented defaults; the push-to-talk menu is
rendered from the same Jac-owned choice list. Rust continues to own the typed
persistence DTO, file I/O, startup defaults, native provider construction,
Keychain access, and OS permission prompts. The Jac client build,
`cargo check --tests --manifest-path src-tauri/Cargo.toml`, and `git diff
--check` pass. Three-run medians are 5.569 s for the Jac client build and
0.206 s for the Rust test-target check. The latest snapshot is 12,350 lines
(Rust 8,846, Swift 142, TypeScript 0, Jac 3,362), 3,813 lines below baseline.
This slice adds UI policy without deleting Rust settings-schema code, so it
increases total source size; settings defaults and legacy migration remain
explicitly outstanding. Runtime dictation and meeting latency are still not
measured because a repeatable consented audio fixture has not been established.

## Preserve an explicitly disabled shortcut — 2026-11-05

Removed the Rust settings-load repair that rewrote an empty hands-free shortcut
to its default. Jac's settings policy accepts an empty shortcut as the explicit
"disabled" value; Rust now only reads/deserializes the saved settings. This
aligns persisted behavior with the settings contract and avoids a hidden native
rewrite. Jac client build, `cargo check --tests --manifest-path
src-tauri/Cargo.toml`, and `git diff --check` pass. Three-run medians are
5.367 s for the Jac client build and 0.185 s for the Rust test-target check.
The source snapshot is 12,342 lines (Rust 8,838, Swift 142, TypeScript 0,
Jac 3,362), 3,821 below baseline; this slice removes 8 Rust lines. The native
default values and backward-compatible missing-field deserialization are still
in `openflow-core` and remain part of the settings migration. No live shortcut
interaction was checked in this slice.

## Cloud cleanup response interpretation — 2026-11-05

OpenAI and Anthropic HTTP adapters now return only the response status and body
to the Jac cleanup bridge. Jac decodes each provider's response shape, rejects
non-success and malformed/empty responses, and lets the existing local-provider
fallback run. The Rust provider adapters no longer parse those response fields
or convert HTTP error bodies into user-facing errors. The Jac client build and
`cargo test --workspace` pass, and `git diff --check` is clean. Three-run
medians: 5.467 s for the Jac client build and 0.665 s for
`cargo check --tests --manifest-path src-tauri/Cargo.toml`; the Rust median
includes a 1.93 s incremental compile on its first sample (the next two were
0.665 s and 0.180 s). The source snapshot is 12,359 lines (Rust 8,818, Swift
142, TypeScript 0, Jac 3,399), 3,804 below baseline. This slice moved 20 lines
out of Rust and added 37 Jac lines, increasing total source by 17 lines; no
per-slice LOC savings are claimed. Live cloud cleanup and provider fallback
were not exercised in the locked app.

## Cloud cleanup request payload construction — 2026-11-05

The Jac cleanup bridge now selects the configured OpenAI or Anthropic model
from settings facts and assembles the provider-specific JSON payload, including
the Anthropic top-level system field and message filtering. Rust cloud adapters
no longer store model names or decide request-body shape; they serialize and
send the Jac-provided body with credentials, endpoint, headers, and timeout.
Publik keeps its separately metered contract and local inference still receives
the Jac message list and token budget. Jac client build and
`cargo test --workspace` pass (41 passed, 1 ignored), and `git diff --check` is
clean. `cargo fmt --all --check` reports extensive formatting differences in
pre-existing dirty files across the workspace; it was not applied to avoid
reformatting unrelated work. The Jac MCP validator remains unavailable because
it errors in runtime setup (`JacRuntime` has no `setup` attribute). Linux
cross-check stops before project compilation because cross pkg-config has no
Linux sysroot; Windows cross-check stops in `ring` because MinGW GCC is absent.
Three-run medians: 5.972 s for the Jac client build and 0.659 s for the Rust
test-target check (its first sample includes a 2.35 s incremental compile; the
next two were 0.61 s and 0.15 s). The latest snapshot is 12,384 lines (Rust
8,811, Swift 142, TypeScript 0, Jac 3,431), 3,779 below baseline. This slice
removed 7 Rust lines and added 32 Jac lines, adding 25 source lines overall;
it does not claim per-slice LOC savings. Live cloud cleanup was not exercised.

## Cloud ASR response interpretation — 2026-11-05

The OpenAI-compatible Cloud ASR adapter now returns only HTTP status/body facts.
The existing Jac cleanup round trip decodes the transcription response before
cleanup, logging, or insertion. If the response cannot be decoded or Jac is
unavailable, the native path returns empty text instead of allowing the JSON
envelope to reach the paste target. The Jac client build and
`cargo test --workspace` pass (41 passed, 1 ignored), and `git diff --check` is
clean. The Jac MCP validator still fails during runtime setup with
`JacRuntime` missing `setup`. Three-run medians: 6.161 s for the Jac client
build and 0.673 s for the Rust test-target check; the first Rust check included
a 1.92 s incremental compile, followed by 0.673 s and 0.195 s. The current
snapshot is 12,417 lines (Rust 8,817, Swift 142, TypeScript 0, Jac 3,458),
3,746 below baseline. This slice adds 6 Rust and 27 Jac lines, increasing total
source by 33 lines; no per-slice LOC reduction is claimed. The Cloud ASR
response path has not yet been exercised against a live provider or installed
app, and a provider error currently resolves as empty transcript text.

## Remove the Rust dictionary domain store — 2026-11-05

The platform adapters now keep the Jac-facing dictionary DTO directly instead
of converting it into a second Rust `DictionaryStore` model. The core Rust
dictionary module and its `DictSource` policy type are removed. Native code
retains only local file access plus the legacy JSON `source` serialization
adapter, so existing manual/auto entries load unchanged and Jac remains the
single owner of edits and source semantics. Added storage compatibility checks
for reading legacy entries and writing the established file shape. Jac client
build and `cargo test --workspace` pass (43 passed, 1 ignored); `git diff
--check` is clean. Three-run medians are 5.521 s for the Jac client build and
0.180 s for the Rust crate check. Source is 12,403 lines (Rust 8,803, Swift
142, TypeScript 0, Jac 3,458), 3,760 lines below the 16,163-line baseline. This
slice reduced total source by 14 nonblank lines versus the prior snapshot; the
per-slice savings come from removing the duplicate domain model. Manual editing
and auto-learn have not yet been exercised against a live dictionary after
this storage change. macOS workspace tests and the app crate check compile; the
Linux and Windows cross-target checks stop in dependency build scripts before
OpenFlow code because this Mac lacks Linux pkg-config/sysroot support and the
MinGW GCC toolchain, respectively.

## Remove Rust dictation action model — 2026-11-05

The macOS adapter now consumes the action objects emitted by Jac's reducer and
dispatches directly to native effects. Removed the Rust `Action`, `BarState`,
`RecordMode`, and `SessionId` types and their core modules; Jac remains the sole
state/action model. Microphone capture, the transient done-state display delay,
ASR, cleanup transport, paste, and OS-specific effects remain in native code.
`cargo test --workspace` passes (43 passed, 1 ignored), and `git diff --check`
is clean. Three-run medians are 7.273 s for the Jac client build and 0.234 s
for the Rust crate check. The snapshot is 12,317 lines (Rust 8,717, Swift 142,
TypeScript 0, Jac 3,458), 3,846 below the 16,163-line baseline. This slice
removed 86 nonblank lines from the previous snapshot. The Jac build timing is
recorded for iteration tracking and does not show a speed improvement over the
previous slice. Windows and Linux action executors follow their own platform
adapters and need their platform toolchains for compile verification.

## Remove the Rust dictation record schema — 2026-11-05

Jac already shaped completed dictation records and owned every metric, but Rust
still imposed a duplicate `SessionRecord` type and typed `StatsStore`. The
platform adapters now pass records as JSON values and use a small native file
adapter solely to read/write the existing `{"sessions": [...]}` envelope. The
Rust core stats module is removed, and new storage tests confirm existing
records and unknown future fields round-trip without Rust schema filtering.
The Jac client build and `cargo test --workspace` pass (45 passed, 1 ignored);
`git diff --check` is clean. Three-run medians: 5.486 s for the Jac client
build and 0.177 s for the Rust test-target check. Source is 12,332 lines (Rust
8,732, Swift 142, TypeScript 0, Jac 3,458), 3,831 below baseline. This slice
adds 15 net nonblank source lines versus the prior snapshot because the native
JSON adapter and compatibility tests exceed the removed type definitions; it
does not claim per-slice LOC savings. Live record/history rendering remains to
be verified after the schema change. Windows/Linux cross-target checks still
stop in dependency build scripts before app code: this Mac lacks the Linux
sysroot/pkg-config setup and MinGW GCC needed for those targets.

## Remove Rust cleanup and ASR mode enums — 2026-11-05

Settings now carry the Jac-owned cleanup mode, cleanup level, and ASR mode
values as strings. Removed the Rust `CleanupMode`, `CleanupLevel`, and
`AsrMode` enums, the unused cleanup-level field on `CleanupContext`, and the
Rust-to-Jac string conversion helpers. Native settings and provider adapters
forward the values as boundary data; Jac validates user changes and owns their
meaning. The Jac client build and `cargo test --workspace` pass (45 passed, 1
ignored), and `git diff --check` is clean. Three-run medians are 5.414 s for
the Jac client build and 0.178 s for the Rust test-target check. Source is
12,262 lines (Rust 8,662, Swift 142, TypeScript 0, Jac 3,458), 3,901 below the
16,163-line baseline; this slice removed 70 nonblank lines. Rust's default
settings values and missing-field handling remain to be migrated. Windows and
Linux target checks remain blocked before app compilation by missing
cross-compilation toolchains.

## Move persisted settings defaults and legacy-field policy into Jac — 2026-11-05

Jac now owns the product defaults for cleanup mode/level, ASR mode/model,
provider models, start sound, hands-free shortcut, and Publik settings. The
settings DTO reports absent persisted fields as boundary metadata; the Jac
dictation bootstrap fills only absent fields, persists them, then registers
dictation. Explicit empty values remain intact, including an intentionally
disabled hands-free shortcut. The Settings page no longer carries a second
hard-coded defaults map. Native code retains only the early push-to-talk
bootstrap needed by the Linux keyboard listener and the typed persistence/OS
boundary.

`jac build main.jac --client web` passes; `cargo test --workspace` passes (45
passed, 1 ignored), and `git diff --check` is clean. The standalone Jac MCP
validator still fails before source validation with `JacRuntime` missing
`setup`; the local Jac client compiler is the successful source check. Three-run
medians: 5.354 s for the Jac client build and 0.647 s for the Rust test-target
check. These timings do not show a Rust-side iteration speed improvement. The
source snapshot is 12,325 lines (Rust 8,684, Swift 142, TypeScript 0, Jac
3,499), 3,838 below the 16,163-line baseline. This settings slice added 63 net
nonblank lines over the previous snapshot because the Jac defaults and missing
field bridge exceed the removed Rust defaults; no per-slice LOC savings are
claimed. Product defaults and legacy missing-field behavior now live in Jac;
cross-platform runtime verification remains outstanding.

## Remove Rust diagnostic failure enum — 2026-11-05

The Rust `InjectionFailure` type and its string conversion table are removed.
Native adapters now emit the stable diagnostic code and platform as boundary
facts; Jac already owns their interpretation, user-facing copy, and display
timing. The platform fact now distinguishes Linux from macOS, and Jac has
Linux-specific microphone and keyboard-hook guidance instead of presenting it
as Windows. The local Jac client build and `cargo test --workspace` pass (45
passed, 1 ignored), and `git diff --check` is clean. The Jac MCP validator still
fails during runtime setup (`JacRuntime` has no `setup` attribute), before it
checks the source.

Three-run medians: 6.890 s for the Jac client build and 0.921 s for the Rust
test-target check; these do not indicate an iteration-speed improvement. The
snapshot is 12,292 lines (Rust 8,642, Swift 142, TypeScript 0, Jac 3,508),
3,871 below the 16,163-line baseline. This slice removed 42 Rust lines, added
9 Jac lines, and reduced total source by 33 nonblank lines. Linux diagnostic
copy and live diagnostic presentation still need runtime verification.

## Move meeting save orchestration into Jac — 2026-11-05

`meeting_workflow.cl.jac` now owns meeting transcript validation, local model
summary generation, transcript-edit acceptance, meeting record identity and
timestamp creation, and the save sequence. Both the live note window and the
Meetings import/record controls use this workflow. Rust's meeting module now
returns the final local Whisper transcript and appends opaque JSON records to
the existing local archive. Removed the Rust meeting DTO, validation/summary
request bridges, and associated Tauri commands; the MCP archive continues to
read the same JSON array. Newest-first archive ordering now lives in Jac.

The Jac bundle and Tauri test-target check pass. A live local note test captured
generated meeting audio, showed live transcript text, completed final local
transcription, automatically generated a summary, and saved it on this Mac.
The finalized transcript was readable in the note window. Interim live text
contained repeated fragments, while the final transcript was deduplicated; this
shows a live rendering defect that remains to be corrected. A full dictation
hotkey-to-paste check remains unverified because desktop-generated key events
did not route through the native global event tap.

Three-run medians: 7.141 s for the Jac client build and 0.793 s for the Tauri
test-target check. Source is 12,134 lines (Rust 8,482, Swift 142, TypeScript 0,
Jac 3,510), 4,029 below the original baseline. This slice removed 160 Rust
lines, added 2 Jac lines, and saved 158 net source lines. These build timings
measure developer iteration, not dictation latency. See the
`jac-client-meeting-workflow`, `rust-check-meeting-workflow`, and latest
`source_snapshot` rows in the metrics CSV.

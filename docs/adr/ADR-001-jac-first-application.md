# ADR-001: Move OpenFlow application code to Jac

**Status:** Accepted for staged migration  
**Date:** 2026-11-04  
**Decider:** Vatsal

## Context

OpenFlow is a local-first Tauri desktop app with Rust business logic and OS
integration, a React/TypeScript UI, and a small Swift ScreenCaptureKit bridge.
The user wants the application rewritten in Jac, retaining only native layers
needed for macOS and other low-level device/model integrations. The existing
checkout contains substantial uncommitted work, which must remain intact while
the migration proceeds.

The local Jac toolchain is 0.13.5 with `jac-client` 0.3.11. Jac's desktop
target generates a Tauri shell; Jac's client target generates a React-based
frontend. The current native Tauri host already owns OpenFlow-specific
permissions, windows, global shortcuts, audio capture, model workers, signing,
and packaging, so replacing it wholesale would discard proven platform work.

## Decision

Migrate application policy and product UI to Jac in small, buildable slices.
Keep the existing Tauri host and native services as adapters; do not introduce a
background HTTP server into the installed app. Determine the least-complex
local Jac-to-Tauri bridge with a small prototype before routing production
commands through it.

### Rewrite in Jac

- `crates/openflow-core`: state machine, settings policy, cleanup prompts and
  gates, dictionary behavior, statistics, and cross-platform data types.
- `crates/openflow-cleanup`: provider selection, request construction, response
  handling, and transcript-formatting policy.
- Product logic in `src-tauri/src`: meeting lifecycle and archive operations,
  calendar synchronization policy, MCP tools, model-download policy, diagnostics
  policy, auto-learn rules, and publik/account flows.
- All product UI currently in `ui/src`: Hub, overlay, meeting prompt and note
  view, and shared UI logic. Preserve the existing visual design and CSS while
  translating behavior to Jac client components.
- Cross-platform orchestration currently in `src-tauri/src/win.rs` and
  `linux.rs`, except for calls that must remain low-level OS adapters.

### Keep native or external

- Tauri window creation, app activation, permissions, accessibility, keychain,
  OS audio session handling, global hotkeys, text insertion, and signed bundle
  setup remain thin native adapters.
- Microphone and system-audio capture stay native to preserve device callbacks
  and low-latency capture. Keep the ScreenCaptureKit Swift bridge where macOS
  requires it.
- Whisper.cpp and llama.cpp remain native inference engines. Jac may own their
  lifecycle and product policy, but model kernels stay in their optimized
  native libraries.
- Keep platform build scripts, manifests, entitlements, and generated assets in
  the formats required by their toolchains.

## Migration sequence

1. Record the current source-size and developer-build baselines; create a Jac
   client build probe that targets the existing Tauri asset pipeline. **Done:**
   the Jac web target emitted a static bundle under `jac/.jac/client/dist`; a
   later measurement builds the overlay component.
2. Move pure, non-latency-sensitive cleanup policy and core data rules to Jac;
   establish a narrow bridge and compare behavior before removing Rust copies.
   **Started:** dictionary merge, persistence, and prefilter logic is in
   `jac/openflow/core/dictionary.jac`; Home and Insights calculate stats in Jac
   from local session records; cleanup levels and deterministic gates are
   defined in `jac/cleanup_policy.cl.jac`. The Jac Settings UI consumes the
   shared cleanup-level data. Dictionary and cleanup ports are not yet on the
   installed dictation path, and Rust stats remain for the current React runtime.
3. Move meeting/calendar/MCP and remaining product service logic to Jac while
   preserving local storage and stdio MCP behavior. **Started:** read/search
   and stdio MCP logic is ported in `jac/meetings_store.jac` and
   `jac/meetings_mcp.jac`; the duplicate Rust MCP executable was removed and
   `scripts/meetings-mcp.sh` launches the Jac implementation. The optional local
   inference worker remains a native adapter; bundling the Jac runtime for MCP
   clients and migrating meeting/calendar policy remain open. Meeting transcript
   formatting and summary policy are now bridged to Jac; the local writer and
   audio/ASR lifecycle remain native adapters.
4. Convert Hub, overlay, and meeting UI to Jac client components, keeping
   app-specific CSS and Tauri window semantics. **Started:** the Jac navigation
   shell, Home, Insights, Dictionary, Notes, Settings, Help, Meetings Hub, overlay, and
   meeting note/reminder windows build from Jac; the entry selects surfaces by
   Tauri window label. The Jac assets are integrated in Tauri and were visually
   checked in the installed Apple Development-signed package; notarized release
   signing remains open.
5. Replace remaining portable Rust product logic, then remove obsolete Rust
   crates and dependencies. Keep native adapter modules only where the app
   needs them.
6. Rebuild and inspect the signed app; compare runtime and development metrics
   against the recorded baselines before calling the migration complete.

## Measurement rules

- Source size is **nonblank physical source lines** under product source
  directories. Generated output, vendored dependencies, scripts, and docs are
  excluded. This is a consistent size proxy, not a claim that every line has
  equal complexity.
- Developer speed uses successful command wall time, with warm medians from at
  least three runs and clean-build timings recorded separately.
- Runtime speed uses the same fixed audio clips, model files, device, and
  settings for every comparison. Record dictation end-to-end and meeting
  live-transcript latency separately; do not claim a speedup without a measured
  comparison.
- The migration tracker is `docs/jac-migration-metrics.csv`; use
  `scripts/migration_metrics.jac` to append source snapshots and repeatable
  command timings.

## Consequences

- Jac becomes the source language for app behavior and product UI, while Tauri
  and native inference/audio components continue to provide desktop integration.
- The installed app must remain local-first and launch without a developer
  server. Build and signing steps need to bundle any Jac runtime required by the
  chosen bridge.
- Jac's Python execution model can make portable orchestration easier to
  express, but it does not inherently make it faster than Rust. Latency-sensitive
  code stays native and all speed claims come from the tracker.
- The migration is not complete until old Rust/TypeScript implementations are
  removed or explicitly classified as native adapters and the final installed
  app passes its existing release checks.

//! Shared Rust boundary types and adapters for platform effects.
//!
//! Product policy and the dictation state machine live in Jac. This crate keeps
//! the native interfaces for ASR, provider cleanup transport, OS diagnostics,
//! and settings persistence.

pub mod asr;
pub mod cleanup;
pub mod settings;

pub use asr::{AsrEngine, AsrEngineId, Transcript};
pub use cleanup::CleanupContext;
pub use settings::{Settings, PUBLIK_DISCLOSURE_VERSION};

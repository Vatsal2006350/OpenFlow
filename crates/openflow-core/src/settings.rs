//! User settings, persisted as JSON. Drives the cleanup engine (which provider,
//! how aggressive) and other behavior. Kept dependency-light so it lives in core.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Persisted user configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Cleanup selection is interpreted by Jac; native code only transports it.
    pub cleanup_mode: String,
    /// Cleanup intensity is interpreted by Jac; native code only transports it.
    pub cleanup_level: String,
    pub openai_model: String,
    /// API root for the "OpenAI" cleanup mode, e.g. `https://openrouter.ai/api/v1`
    /// to route through OpenRouter instead of OpenAI directly (same wire format).
    /// Empty string (the default) means OpenAI's own endpoint.
    pub openai_base_url: String,
    pub anthropic_model: String,
    /// ASR selection is interpreted by Jac; native code only transports it.
    pub asr_mode: String,
    /// API root for cloud ASR, e.g. `https://api.groq.com/openai/v1`
    /// for Groq's fast Whisper endpoint. Empty string means OpenAI's own
    /// endpoint. Uses the same stored key as the "OpenAI" cleanup mode.
    pub asr_base_url: String,
    pub asr_model: String,
    /// Play the record-start ping.
    pub sound_on_start: bool,
    /// The push-to-talk key, stored as an rdev::Key variant string (e.g.
    /// "ControlRight"). Jac selects its product default. The native DTO keeps a
    /// platform-safe bootstrap value because Linux's keyboard listener starts
    /// before Jac; macOS/Windows use their native key constants today.
    pub push_to_talk_key: String,
    /// The global hotkey that toggles HANDS-FREE (locked) dictation — press once
    /// to start talking, press again to stop, with no key held down. An
    /// accelerator string in Tauri's format (e.g. "CmdOrCtrl+Shift+Space"). Jac
    /// selects the platform default. Holding Fn
    /// (push-to-talk) and double-tapping Fn (hands-free) still work regardless.
    /// An empty string disables the hands-free hotkey.
    pub hands_free_hotkey: String,
    /// The version of the publik cost + data-path disclosure the user has
    /// accepted in this app. 0 = never accepted. Bump
    /// [`PUBLIK_DISCLOSURE_VERSION`] when the wording changes and the card
    /// shows again once.
    pub publik_disclosure_version: u32,
    /// Random v4 UUID minted on first provisioning (not a hardware id). It is
    /// the idempotency key of `POST /installs`, so a retried provision never
    /// mints a second key for the same install. Empty until first provision.
    pub publik_install_id: String,
    /// API root handed back by `POST /installs` (`base_url`). Honoured over the
    /// compiled default; empty means "use the default". Not a UI field.
    pub publik_base_url: String,
    /// The fast-tier alias handed back by `POST /installs` (`models.fast`).
    /// Empty means the compiled default alias. Never an upstream model slug.
    pub publik_model: String,
    /// Claim link for this install (`claim_url`), kept so the Settings card can
    /// offer "Link this computer" after a relaunch without a network round trip.
    /// Empty once claimed or before provisioning.
    pub publik_claim_url: String,
    /// Free use granted by `POST /installs` (`starter_micros`), kept so the
    /// "running low" banner knows what 20% of it is. Since publik migration
    /// 0059 (founder, 2026-09-28) a new computer starts at $0.00, so this is 0
    /// for every install minted without an account; the one free thing is
    /// $0.05 of use when the computer is linked to a publik account, once per
    /// account. 0 also means "unknown": the banner then assumes that $0.05.
    pub publik_starter_micros: i64,
    /// The first-run publik card (balance line, why it costs money, "Link this
    /// computer & pick a plan") is owed to the user: set by provisioning,
    /// cleared by "Later" or the button. Persisted so an app that quits before
    /// the card was seen shows it on the next launch (CONTRACT §12.4: never a
    /// silent starter).
    pub publik_cta_pending: bool,
    /// Boundary metadata: lets Jac distinguish a missing legacy field from an
    /// intentionally empty value. Never serialized into the settings file.
    #[serde(skip)]
    pub jac_missing_fields: Vec<String>,
}

pub const SETTINGS_FIELDS: &[&str] = &[
    "cleanup_mode", "cleanup_level", "openai_model", "openai_base_url",
    "anthropic_model", "asr_mode", "asr_base_url", "asr_model",
    "sound_on_start", "push_to_talk_key", "hands_free_hotkey",
    "publik_disclosure_version", "publik_install_id", "publik_base_url",
    "publik_model", "publik_claim_url", "publik_starter_micros",
    "publik_cta_pending",
];

/// Version of the in-app publik disclosure copy. Bumping it re-shows the card.
pub const PUBLIK_DISCLOSURE_VERSION: u32 = 1;

fn native_bootstrap_push_to_talk_key() -> String {
    #[cfg(target_os = "macos")]
    { "AltLeft".to_string() }
    #[cfg(not(target_os = "macos"))]
    { "ControlRight".to_string() }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cleanup_mode: String::new(),
            cleanup_level: String::new(),
            openai_model: String::new(),
            openai_base_url: String::new(),
            anthropic_model: String::new(),
            asr_mode: String::new(),
            asr_base_url: String::new(),
            asr_model: String::new(),
            sound_on_start: false,
            push_to_talk_key: native_bootstrap_push_to_talk_key(),
            hands_free_hotkey: String::new(),
            publik_disclosure_version: 0,
            publik_install_id: String::new(),
            publik_base_url: String::new(),
            publik_model: String::new(),
            publik_claim_url: String::new(),
            publik_starter_micros: 0,
            publik_cta_pending: false,
            jac_missing_fields: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
            .and_then(|value| {
                let object = value.as_object()?;
                let missing = SETTINGS_FIELDS
                    .iter()
                    .filter(|field| !object.contains_key(**field))
                    .map(|field| (*field).to_string())
                    .collect::<Vec<_>>();
                serde_json::from_value::<Self>(value)
                    .ok()
                    .map(|mut settings| {
                        settings.jac_missing_fields = missing;
                        settings
                    })
            })
            .unwrap_or_else(|| {
                let mut settings = Self::default();
                settings.jac_missing_fields = SETTINGS_FIELDS.iter().map(|x| (*x).to_string()).collect();
                settings
            });
        settings.jac_missing_fields.sort();
        settings
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_neutral_native_bootstrap_values() {
        let s = Settings::default();
        assert!(s.cleanup_mode.is_empty());
        assert!(s.cleanup_level.is_empty());
        assert!(s.asr_mode.is_empty());
        assert!(!s.sound_on_start);
    }

    #[test]
    fn round_trips_json() {
        let s = Settings {
            cleanup_mode: "local".to_string(),
            jac_missing_fields: vec!["cleanup_level".to_string()],
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("jac_missing_fields"));
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cleanup_mode, "local");
        assert!(back.jac_missing_fields.is_empty());
    }

    #[test]
    fn native_push_to_talk_bootstrap_binding_is_platform_appropriate() {
        #[cfg(target_os = "macos")]
        assert_eq!(Settings::default().push_to_talk_key, "AltLeft");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(Settings::default().push_to_talk_key, "ControlRight");

        // A user who set their own hands-free shortcut keeps it.
        let mut custom = Settings::default();
        custom.hands_free_hotkey = "Alt+Space".to_string();
        let back: Settings = serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
        assert_eq!(back.hands_free_hotkey, "Alt+Space");
    }

    #[test]
    fn partial_legacy_settings_are_passed_to_jac_with_missing_field_metadata() {
        let old_json = r#"{
            "cleanup_level": "light",
            "openai_model": "gpt-4o-mini",
            "anthropic_model": "claude-haiku-4-5",
            "hands_free_hotkey": "",
            "sound_on_start": true
        }"#;
        let path = std::env::temp_dir().join(format!("openflow-settings-{}.json", std::process::id()));
        std::fs::write(&path, old_json).unwrap();
        let loaded = Settings::load(&path);
        let _ = std::fs::remove_file(&path);
        assert!(loaded.cleanup_mode.is_empty());
        assert!(loaded.jac_missing_fields.contains(&"cleanup_mode".to_string()));
        assert!(loaded.jac_missing_fields.contains(&"publik_install_id".to_string()));
        assert!(!loaded.jac_missing_fields.contains(&"hands_free_hotkey".to_string()));
        assert!(loaded.hands_free_hotkey.is_empty());
        assert!(!loaded.jac_missing_fields.contains(&"sound_on_start".to_string()));
        assert_eq!(loaded.publik_disclosure_version, 0);
        assert_eq!(loaded.publik_install_id, "");
        assert_eq!(loaded.publik_base_url, "");
        assert_eq!(loaded.publik_model, "");
        assert_eq!(loaded.publik_claim_url, "");
        assert_eq!(loaded.publik_starter_micros, 0);
        assert!(!loaded.publik_cta_pending, "no card is owed before anything was provisioned");
    }

    #[test]
    fn publik_mode_round_trips() {
        let s = Settings {
            cleanup_mode: "publik".to_string(),
            publik_disclosure_version: PUBLIK_DISCLOSURE_VERSION,
            publik_install_id: "3f1c9b5e-7a2d-4c8e-9f0b-1d2e3f4a5b6c".to_string(),
            publik_base_url: "https://publikhq.com/api/v1".to_string(),
            // An install minted already bound to an account: the one $0.05 grant.
            publik_starter_micros: 50_000,
            publik_cta_pending: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"cleanup_mode\":\"publik\""), "{json}");
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cleanup_mode, "publik");
        assert_eq!(back.publik_install_id, s.publik_install_id);
        assert_eq!(back.publik_base_url, s.publik_base_url);
        assert_eq!(back.publik_starter_micros, 50_000);
        assert!(back.publik_cta_pending, "the owed first-run card survives a relaunch");
    }

    #[test]
    fn product_defaults_are_not_owned_by_the_native_dto() {
        assert!(Settings::default().cleanup_mode.is_empty());
    }
}

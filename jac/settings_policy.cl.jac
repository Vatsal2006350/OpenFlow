"""Validation and normalization rules for user-editable settings."""

cl import from .cleanup_policy { cleanup_levels }

cl def:pub settings_defaults(platform: str) -> dict {
    macos = platform == "macos";
    return {
        "cleanup_mode": "local", "cleanup_level": "light",
        "openai_model": "gpt-4o-mini", "openai_base_url": "",
        "anthropic_model": "claude-haiku-4-5", "asr_mode": "local",
        "asr_base_url": "", "asr_model": "whisper-large-v3-turbo",
        "sound_on_start": True,
        "push_to_talk_key": "AltLeft" if macos else "ControlRight",
        "hands_free_hotkey": "Alt+Space" if macos else "CmdOrCtrl+Shift+Space",
        "publik_disclosure_version": 0, "publik_install_id": "",
        "publik_base_url": "", "publik_model": "", "publik_claim_url": "",
        "publik_starter_micros": 0, "publik_cta_pending": False
    };
}

cl def:pub fill_missing_settings(settings: dict, missing_fields: list[object], platform: str) -> dict {
    result: dict = dict(settings);
    defaults = settings_defaults(platform);
    for field in missing_fields {
        key = str(field);
        if key in defaults { result[key] = defaults[key]; }
    }
    result.pop("_jac_missing_fields", None);
    result.pop("_jac_platform", None);
    return result;
}

cl def:pub push_to_talk_choices() -> list[dict] {
    return [
        {"value": "AltLeft", "label": "Left Option"},
        {"value": "AltRight", "label": "Right Option"},
        {"value": "ControlLeft", "label": "Left Control"},
        {"value": "ControlRight", "label": "Right Control"},
        {"value": "ShiftLeft", "label": "Left Shift"},
        {"value": "ShiftRight", "label": "Right Shift"},
        {"value": "CapsLock", "label": "Caps Lock"},
        {"value": "F6", "label": "F6"},
        {"value": "F7", "label": "F7"},
        {"value": "F8", "label": "F8"}
    ];
}

cl def:pub normalize_setting_update(field: str, value: object) -> dict {
    if field == "cleanup_mode" {
        allowed = ["raw", "local", "publik", "open_ai", "anthropic"];
        candidate = str(value);
        return {"accepted": candidate in allowed, "value": candidate};
    }
    if field == "cleanup_level" {
        candidate = str(value);
        for level in cleanup_levels() {
            if candidate == str(level.get("value", "")) { return {"accepted": True, "value": candidate}; }
        }
        return {"accepted": False, "value": ""};
    }
    if field == "asr_mode" {
        candidate = str(value);
        return {"accepted": candidate in ["local", "cloud"], "value": candidate};
    }
    if field == "push_to_talk_key" {
        candidate = str(value);
        for choice in push_to_talk_choices() {
            if candidate == str(choice["value"]) { return {"accepted": True, "value": candidate}; }
        }
        return {"accepted": False, "value": ""};
    }
    if field == "sound_on_start" { return {"accepted": True, "value": bool(value)}; }
    text_fields = ["openai_model", "anthropic_model", "asr_model", "openai_base_url", "asr_base_url", "hands_free_hotkey"];
    if field in text_fields {
        candidate = str(value).strip();
        if field == "openai_model" and not candidate { candidate = "gpt-4o-mini"; }
        if field == "anthropic_model" and not candidate { candidate = "claude-haiku-4-5"; }
        if field == "asr_model" and not candidate { candidate = "whisper-large-v3-turbo"; }
        if field == "openai_base_url" or field == "asr_base_url" { candidate = candidate.rstrip("/"); }
        return {"accepted": True, "value": candidate};
    }
    return {"accepted": False, "value": ""};
}

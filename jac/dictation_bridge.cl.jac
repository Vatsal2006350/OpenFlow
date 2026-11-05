"""Connect native hotkey events to the Jac dictation reducer."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .dictation_policy { reduce_dictation }
cl import from .settings_policy { fill_missing_settings }

cl def:pub DictationBridge() -> JsxElement {
    has machine: dict = {"phase": "idle", "mode": "", "session": 0, "started_ms": 0, "warned": False, "last_end_ms": -1000000, "tap_up_ms": 0, "next_session": 1, "sound_on_start": True};
    async def process(event: dict) {
        payload: dict = dict(event.get("payload", {}));
        actions = reduce_dictation(machine, payload);
        machine = dict(machine);
        if len(actions) > 0 {
            try { await invoke("apply_jac_dictation_actions", {"actions": actions}); }
            except Exception { }
        }
    }
    def apply_settings(event: dict) {
        settings: dict = dict(event.get("payload", {}));
        machine["sound_on_start"] = bool(settings.get("sound_on_start", True));
    }
    useEffect(lambda {
        async def subscribe() {
            try {
                await listen("openflow://settings/updated", lambda (event: dict) { apply_settings(event); });
                settings: dict = dict(await invoke("get_settings"));
                missing_fields: list[object] = list(settings.get("_jac_missing_fields", []));
                if len(missing_fields) > 0 {
                    platform = str(settings.get("_jac_platform", "macos"));
                    settings = fill_missing_settings(settings, missing_fields, platform);
                    try { await invoke("set_settings", {"settings": settings}); }
                    except Exception { }
                } else {
                    settings.pop("_jac_missing_fields", None);
                    settings.pop("_jac_platform", None);
                }
                machine["sound_on_start"] = bool(settings.get("sound_on_start", True));
                await listen("openflow://dictation/input", lambda (event: dict) { process(event); });
                await invoke("register_jac_dictation");
            } except Exception { }
        }
        subscribe();
        return lambda { invoke("unregister_jac_dictation"); };
    }, []);
    return <span aria-hidden="true" className="jac-dictation-bridge" />;
}

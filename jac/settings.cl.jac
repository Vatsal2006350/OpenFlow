"""Jac settings screen; sensitive credentials and OS permissions stay native."""

cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .cleanup_policy { cleanup_levels }
cl import from .publik_policy { publik_balance_label, publik_week_label, publik_last_charge_label, publik_first_run_view, publik_plan_button, publik_action_url, publik_why_costs, publik_provision_error }
cl import from .settings_policy { normalize_setting_update, push_to_talk_choices, fill_missing_settings }

cl def:pub SettingsPage() -> JsxElement {
    has settings: dict = {};
    has status: dict = {};
    has publik_status: dict = {};
    has loading: bool = True;
    has key_provider: str = "openai";
    has key_value: str = "";
    has key_message: str = "";
    has key_error: str = "";
    has settings_error: str = "";
    has busy: bool = False;
    has disclosure: bool = False;
    has disclosure_error: str = "";
    has publik_error: str = "";
    has confirm_forget: bool = False;
    has why_publik: bool = False;

    async def refresh() {
        try {
            settings = dict(await invoke("get_settings"));
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
            status = dict(await invoke("get_status"));
        } except Exception {}
        try { publik_status = dict(await invoke("publik_refresh_wallet", {"force": False})); }
        except Exception { try { publik_status = dict(await invoke("get_publik_status")); } except Exception { } }
        loading = False;
    }
    useEffect(lambda { refresh(); }, []);

    async def save_field(field: str, value: object) {
        settings_error = "";
        decision: dict = normalize_setting_update(field, value);
        if not bool(decision.get("accepted", False)) {
            settings_error = "That setting value is not supported.";
            return;
        }
        next_settings: dict = dict(settings);
        next_settings[field] = decision["value"];
        settings = next_settings;
        try { await invoke("set_settings", {"settings": next_settings}); }
        except Exception { settings_error = "Could not save settings on this device."; }
    }
    async def grant(command: str) {
        try { await invoke(command); }
        except Exception {}
        await refresh();
    }
    async def store_key() {
        if not key_value.strip() or busy { return; }
        busy = True;
        key_message = "";
        key_error = "";
        try {
            await invoke("set_api_key", {"provider": key_provider, "key": key_value});
            key_value = "";
            key_message = "Saved securely in the macOS Keychain.";
            status = dict(await invoke("get_status"));
        } except Exception { key_error = "Could not save the key to the system credential store."; }
        busy = False;
    }
    async def enable_publik() {
        busy = True;
        disclosure_error = "";
        try {
            publik_status = dict(await invoke("publik_accept_disclosure"));
            settings = dict(await invoke("get_settings"));
            status = dict(await invoke("get_status"));
            disclosure = False;
        } except Exception as err { disclosure_error = publik_provision_error(str(err)); }
        busy = False;
    }

    async def settle_publik_first_run(open_link: bool) {
        if busy { return; }
        busy = True;
        try {
            if open_link { await open_publik_link("first_run"); }
            else { publik_status = dict(await invoke("publik_dismiss_first_run")); }
            publik_status = dict(await invoke("get_publik_status"));
        } except Exception { }
        busy = False;
    }

    async def refresh_publik_wallet() {
        if busy { return; }
        busy = True;
        publik_error = "";
        try { publik_status = dict(await invoke("publik_refresh_wallet", {"force": True})); }
        except Exception as err { publik_error = str(err); }
        busy = False;
    }

    async def open_publik_link(kind: str) {
        try {
            url = publik_action_url(kind, publik_status);
            await invoke("publik_open_url", {"url": url});
            if kind == "first_run" { publik_status = dict(await invoke("publik_dismiss_first_run")); }
        }
        except Exception as err { publik_error = str(err); }
    }

    async def forget_publik_key() {
        if busy { return; }
        busy = True;
        try { publik_status = dict(await invoke("publik_forget_key")); }
        except Exception as err { publik_error = str(err); }
        confirm_forget = False;
        busy = False;
    }

    async def use_own_key() {
        disclosure = False;
        await save_field("cleanup_mode", "open_ai");
        document.getElementById("jac-byo-keys").scrollIntoView({"behavior": "smooth", "block": "start"});
    }

    cleanup_mode = str(settings.get("cleanup_mode", "local"));
    cleanup_level = str(settings.get("cleanup_level", "light"));
    asr_mode = str(settings.get("asr_mode", "local"));
    first_run = publik_first_run_view(publik_status);
    show_first_run = bool(first_run) and not disclosure;
    publik_ready = bool(publik_status.get("has_key", False)) and not bool(publik_status.get("exhausted", False)) and not bool(publik_status.get("disconnected", False)) and not bool(publik_status.get("unreachable", False));
    publik_has_key = bool(publik_status.get("has_key", False));
    publik_exhausted = bool(publik_status.get("exhausted", False));
    publik_disconnected = bool(publik_status.get("disconnected", False));
    publik_unreachable = bool(publik_status.get("unreachable", False));
    publik_anonymous = str(publik_status.get("claim_state", "")) != "claimed";
    publik_plan_cta = publik_plan_button(publik_status);
    publik_balance_text = publik_balance_label(publik_status.get("balance_micros", None));
    publik_week_text = publik_week_label(publik_status.get("week_used_micros", None), publik_status.get("week_budget_micros", None));
    publik_last_charge_text = publik_last_charge_label(publik_status.get("last_charge_micros", None));
    level_options = cleanup_levels();
    level_rows: list[JsxElement] = [];
    for level in level_options {
        level_value = str(level["value"]);
        level_rows.append(<button key={level_value} className={"jac-settings-level active" if cleanup_level == level_value else "jac-settings-level"} onClick={lambda { save_field("cleanup_level", level_value); }}><strong>{str(level["label"])}</strong><small>{str(level["hint"])}</small></button>);
    }
    ptt_key_rows: list[JsxElement] = [];
    for choice in push_to_talk_choices() {
        ptt_key_rows.append(<option key={str(choice["value"])} value={str(choice["value"])}>{str(choice["label"])}</option>);
    }
    return <section className="jac-settings">
        <div className="jac-home-eyebrow">Your preferences</div>
        <h1>Settings</h1>
        {loading and <div className="jac-settings-card">Loading settings saved on this Mac…</div>}
        {not loading and <div className="jac-settings-stack">
            {settings_error and <small className="jac-settings-error">{settings_error}</small>}
            <article className="jac-settings-card">
                <h2>Speech-to-Text</h2><p>Choose where speech is transcribed.</p>
                <div className="jac-settings-choice-row">
                    <button className={"jac-settings-choice active" if asr_mode == "local" else "jac-settings-choice"} onClick={lambda { save_field("asr_mode", "local"); }}>Local</button>
                    <button className={"jac-settings-choice active" if asr_mode == "cloud" else "jac-settings-choice"} onClick={lambda { save_field("asr_mode", "cloud"); }}>Cloud</button>
                </div>
                <small>{"On-device Whisper. Works offline with a downloaded speech model." if asr_mode == "local" else "Uses your OpenAI key and the API endpoint below."}</small>
                {asr_mode == "cloud" and <div className="jac-settings-fields">
                    <label>Transcription API base URL<input value={str(settings.get("asr_base_url", ""))} placeholder="https://api.groq.com/openai/v1" onChange={lambda (event: dict) { settings["asr_base_url"] = str(event["target"]["value"]); }} onBlur={lambda { save_field("asr_base_url", str(settings.get("asr_base_url", ""))); }} /></label>
                    <label>Transcription model<input value={str(settings.get("asr_model", "whisper-large-v3-turbo"))} onChange={lambda (event: dict) { settings["asr_model"] = str(event["target"]["value"]); }} onBlur={lambda { save_field("asr_model", str(settings.get("asr_model", ""))); }} /></label>
                </div>}
            </article>

            <article className="jac-settings-card">
                <h2>Cleanup Engine</h2><p>Where your transcript is polished before it is typed.</p>
                <div className="jac-settings-choice-row jac-settings-wrap">
                    <button className={"jac-settings-choice active" if cleanup_mode == "raw" else "jac-settings-choice"} onClick={lambda { save_field("cleanup_mode", "raw"); }}>Raw</button>
                    <button className={"jac-settings-choice active" if cleanup_mode == "local" else "jac-settings-choice"} onClick={lambda { save_field("cleanup_mode", "local"); }}>Local</button>
                    <button className={"jac-settings-choice active" if cleanup_mode == "open_ai" else "jac-settings-choice"} onClick={lambda { save_field("cleanup_mode", "open_ai"); }}>OpenAI</button>
                    <button className={"jac-settings-choice active" if cleanup_mode == "anthropic" else "jac-settings-choice"} onClick={lambda { save_field("cleanup_mode", "anthropic"); }}>Anthropic</button>
                    <button className={"jac-settings-choice active" if cleanup_mode == "publik" else "jac-settings-choice"} onClick={lambda { if cleanup_mode == "publik" { return; } if bool(publik_status.get("disclosure_needed", True)) or not publik_has_key { disclosure_error = ""; publik_error = ""; disclosure = True; } else { save_field("cleanup_mode", "publik"); } }}>publik API</button>
                </div>
                <small>{"Paste speech exactly as recognized." if cleanup_mode == "raw" else "On-device model; your transcript stays local." if cleanup_mode == "local" else "Cloud cleanup uses the selected provider and its saved credentials."}</small>
                {disclosure and <div className="jac-publik-disclosure"><strong>Before turning on publik API</strong><p>Your transcript text, never audio, is sent through publik to a shared model. It is priced per use at half the model’s published list price. The service does not train on or store your transcript.</p>{disclosure_error and <small className="jac-settings-error">{disclosure_error}</small>}{publik_error and <small className="jac-settings-error">{publik_error}</small>}<div><button className="jac-primary-button" disabled={busy or not bool(publik_status.get("can_provision", False)) and not publik_has_key} onClick={lambda { enable_publik(); }}>{"Setting up…" if busy else "Agree and turn on publik API"}</button><button className="jac-secondary-button" onClick={lambda { disclosure = False; }}>Cancel</button><button className="jac-secondary-button" onClick={lambda { use_own_key(); }}>Use my own key</button></div></div>}
                {cleanup_mode == "local" and not bool(status.get("local_model_present", False)) and not disclosure and <div className="jac-publik-inline"><span>Local cleanup needs a model that is not installed, so text is pasted as spoken.</span> {bool(publik_status.get("available", False)) and <button className="jac-inline-link" onClick={lambda { disclosure = True; }}>Turn on publik API →</button>} <span>or add your own key below.</span></div>}
                {cleanup_mode == "open_ai" and <div className="jac-settings-fields">
                    <label>OpenAI-compatible base URL<input value={str(settings.get("openai_base_url", ""))} placeholder="https://openrouter.ai/api/v1" onChange={lambda (event: dict) { settings["openai_base_url"] = str(event["target"]["value"]); }} onBlur={lambda { save_field("openai_base_url", str(settings.get("openai_base_url", ""))); }} /></label>
                    <label>Cleanup model<input value={str(settings.get("openai_model", "gpt-4o-mini"))} onChange={lambda (event: dict) { settings["openai_model"] = str(event["target"]["value"]); }} onBlur={lambda { save_field("openai_model", str(settings.get("openai_model", ""))); }} /></label>
                </div>}
                {cleanup_mode == "anthropic" and <div className="jac-settings-fields"><label>Anthropic model<input value={str(settings.get("anthropic_model", "claude-haiku-4-5"))} onChange={lambda (event: dict) { settings["anthropic_model"] = str(event["target"]["value"]); }} onBlur={lambda { save_field("anthropic_model", str(settings.get("anthropic_model", ""))); }} /></label></div>}
            </article>

            {show_first_run and <article className="jac-settings-card jac-publik-first-run">
                <strong>publik API is on</strong>
                <div className="jac-publik-first-run-balance">{str(first_run.get("balance_line", ""))}</div>
                <p>{str(first_run.get("justification", ""))}</p>
                <div className="jac-publik-first-run-actions">
                    <button className="jac-primary-button" disabled={busy} onClick={lambda { settle_publik_first_run(True); }}>{str(first_run.get("cta_label", "Link this computer & pick a plan"))}</button>
                    <button className="jac-secondary-button" disabled={busy} onClick={lambda { settle_publik_first_run(False); }}>{str(first_run.get("later_label", "Later"))}</button>
                </div>
                <small>Later changes nothing. You can link this computer or pick a plan any time from Settings.</small>
            </article>}

            {cleanup_mode == "publik" and not show_first_run and <article className="jac-settings-card" id="jac-publik-account">
                <h2>publik API</h2>
                <div className="jac-publik-account-state"><span className={"jac-status-dot" + (publik_ready and " ready" or "")}></span><strong>{"— not set up" if not publik_has_key else "— disconnected" if publik_disconnected else "— needs credit" if publik_exhausted else "— unreachable" if publik_unreachable else "— ready"}</strong>{publik_balance_text and <span>· {publik_balance_text}</span>}{publik_week_text and <span>· {publik_week_text}</span>}{publik_last_charge_text and <span>· {publik_last_charge_text}</span>}</div>
                {publik_has_key and publik_exhausted and <p className="jac-publik-error-copy"><strong>{"publik API needs a balance." if publik_anonymous else "publik API needs a plan or a pack."}</strong> {"Link this computer for $0.05 of free use or pick a plan, or use your own key." if publik_anonymous else "Add a plan or a pack, or use your own key."} Dictation still works; text is pasted without cleanup.</p>}
                {publik_disconnected and <p className="jac-publik-error-copy"><strong>publik API is disconnected.</strong> This computer was removed from your publik account.</p>}
                {publik_has_key and not publik_disconnected and publik_unreachable and <p className="jac-publik-muted-copy"><strong>publik API is unreachable right now.</strong> Nothing is being charged. Try again in a minute, or use your own key.</p>}
                {publik_error and <small className="jac-settings-error">{publik_error}</small>}
                <div className="jac-publik-account-actions">
                    {(publik_disconnected or not publik_has_key) and <button className="jac-primary-button" disabled={busy or not bool(publik_status.get("can_provision", False))} onClick={lambda { disclosure_error = ""; disclosure = True; }}>{"Reconnect" if publik_disconnected else "Turn on publik API"}</button>}
                    {publik_has_key and publik_exhausted and <button className="jac-primary-button" onClick={lambda { open_publik_link("top_up"); }}>{"Link this computer & pick a plan" if publik_anonymous else "Add a plan or pack"}</button>}
                    {publik_has_key and not publik_exhausted and not publik_disconnected and <button className="jac-primary-button" onClick={lambda { open_publik_link("plan"); }}>{str(publik_plan_cta.get("label", "Pick a plan"))}</button>}
                    <button className="jac-secondary-button" onClick={lambda { open_publik_link("dashboard"); }}>Usage</button>
                    <button className="jac-secondary-button" onClick={lambda { use_own_key(); }}>Use my own key instead</button>
                    {publik_has_key and not confirm_forget and <button className="jac-secondary-button" onClick={lambda { confirm_forget = True; }}>Forget this key</button>}
                    {publik_has_key and confirm_forget and <button className="jac-secondary-button" disabled={busy} onClick={lambda { forget_publik_key(); }}>Really forget this key?</button>}
                    {publik_has_key and confirm_forget and <button className="jac-secondary-button" onClick={lambda { confirm_forget = False; }}>Cancel</button>}
                    <button className="jac-secondary-button" disabled={busy} onClick={lambda { refresh_publik_wallet(); }}>Refresh balance</button>
                </div>
                {publik_has_key and <div className="jac-publik-why"><button className="jac-inline-link" onClick={lambda { why_publik = not why_publik; }}>{"Why it costs money ▾" if why_publik else "Why it costs money ▸"}</button>{why_publik and <p>{publik_why_costs()}</p>}</div>}
            </article>}

            <article className="jac-settings-card">
                <h2>Auto Cleanup</h2><p>Set how much the transcript may be edited.</p>
                <div className="jac-settings-levels">{level_rows}</div>
            </article>

            <article className="jac-settings-card">
                <h2>Shortcuts</h2>
                <div className="jac-settings-fields">
                    <label>Push-to-talk key<select value={str(settings.get("push_to_talk_key", "AltLeft"))} onChange={lambda (event: dict) { save_field("push_to_talk_key", str(event["target"]["value"])); }}>
                        {ptt_key_rows}
                    </select></label>
                    <label>Hands-free shortcut<input value={str(settings.get("hands_free_hotkey", "Alt+Space"))} placeholder="Alt+Space" onChange={lambda (event: dict) { settings["hands_free_hotkey"] = str(event["target"]["value"]); }} onBlur={lambda { save_field("hands_free_hotkey", str(settings.get("hands_free_hotkey", ""))); }} /><small>Press once to start and again to stop. On macOS, Option+Space is the default.</small></label>
                    <label className="jac-settings-toggle"><span>Play a sound when recording starts</span><input type="checkbox" checked={bool(settings.get("sound_on_start", True))} onChange={lambda (event: dict) { save_field("sound_on_start", bool(event["target"]["checked"])); }} /></label>
                </div>
                <small>Push-to-talk key changes take effect after restarting the app.</small>
            </article>

            <article className="jac-settings-card">
                <h2 id="jac-byo-keys">API Keys</h2><p>Keys are stored by the native system credential service and never shown again.</p>
                <div className="jac-settings-choice-row"><button className={"jac-settings-choice active" if key_provider == "openai" else "jac-settings-choice"} onClick={lambda { key_provider = "openai"; }}>OpenAI</button><button className={"jac-settings-choice active" if key_provider == "anthropic" else "jac-settings-choice"} onClick={lambda { key_provider = "anthropic"; }}>Anthropic</button></div>
                <div className="jac-key-status">{("OpenAI" if key_provider == "openai" else "Anthropic") + (" key is configured." if bool(status.get("has_openai_key" if key_provider == "openai" else "has_anthropic_key", False)) else " key is not set.")}</div>
                <div className="jac-settings-key-row"><input type="password" value={key_value} placeholder="Paste a new API key" autoComplete="new-password" onChange={lambda (event: dict) { key_value = str(event["target"]["value"]); key_message = ""; }} /><button className="jac-primary-button" disabled={busy or not key_value.strip()} onClick={lambda { store_key(); }}>{"Saving…" if busy else "Save securely"}</button></div>
                {key_message and <small className="jac-settings-success">{key_message}</small>}{key_error and <small className="jac-settings-error">{key_error}</small>}
            </article>

            <article className="jac-settings-card">
                <h2>Permissions</h2><p>Grant local access required for dictation. Status refreshes after each request.</p>
                <div className="jac-permission-row"><span><strong>Accessibility</strong><small>{"Required for global hotkeys and text insertion."}</small></span><button className="jac-secondary-button" onClick={lambda { grant("request_accessibility"); }}>{"Granted" if bool(status.get("accessibility", False)) else "Grant"}</button></div>
                <div className="jac-permission-row"><span><strong>Microphone</strong><small>Captures your speech on this device.</small></span><button className="jac-secondary-button" onClick={lambda { grant("request_microphone"); }}>{"Granted" if bool(status.get("microphone", False)) else "Grant"}</button></div>
                <div className="jac-permission-row"><span><strong>Input Monitoring</strong><small>Optional extra reliability for key detection.</small></span><button className="jac-secondary-button" onClick={lambda { grant("request_input_monitoring"); }}>{"Granted" if bool(status.get("input_monitoring", False)) else "Grant"}</button></div>
            </article>
        </div>}
    </section>;
}

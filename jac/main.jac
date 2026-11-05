"""OpenFlow Jac client entry point for staged UI migration."""

cl import from .overlay { Overlay }
cl import from .meeting { MeetingWindow }
cl import from .meetings_hub { MeetingsHub }
cl import from .home { Home }
cl import from .dictionary { Dictionary }
cl import from .notes { Notes }
cl import from .help { Help }
cl import from .coming_soon { ComingSoon }
cl import from .insights { Insights }
cl import from .settings { SettingsPage }
cl import from .cleanup_bridge { CleanupBridge }
cl import from .dictation_bridge { DictationBridge }
cl import from .audio_bridge { AudioPolicyBridge }
cl import from .calendar_bridge { CalendarBridge }
cl import from .autolearn_bridge { AutoLearnBridge }
cl import from .diagnostics_bridge { DiagnosticsBridge }
cl import from .model_bridge { ModelBridge }
cl import from .stats_bridge { StatsBridge }
cl import from .model_policy { model_download_size_label, model_megabytes, model_failure_message }
cl import from .permissions_policy { microphone_permission_hint }
cl import from .publik_policy { publik_low_starter_notice, publik_exhausted_notice }
cl import from "@tauri-apps/api/window" { getCurrentWindow }
cl import from "@tauri-apps/api/core" { invoke }
cl import from "@tauri-apps/api/event" { listen }
cl import from react { useEffect, useState }
cl import "./styles.css";

cl {
    def HubApp() -> JsxElement {
        has page: str = "home";
        has permission_status: dict = {
            "accessibility": False, "microphone": False, "input_monitoring": False,
            "microphone_grant": "not_asked", "charged_to": None,
            "microphone_hint": "Lets OpenFlow hear what you say."
        };
        has permission_loading: bool = True;
        has entered: bool = False;
        has last_error: dict = {};
        has error_dismissed: bool = False;
        has publik_status: dict = {"exhausted_message": None};
        has low_starter_shown: bool = False;
        has low_starter_dismissed: bool = False;
        has asr_model: dict = {"state": "loading"};

        def mark_entered() {
            entered = True;
            try { window.localStorage.setItem("openflow_onboarding_done", "1"); } except Exception { }
        }

        def apply_permissions(update: dict) {
            next_status: dict = dict(permission_status);
            next_status["accessibility"] = bool(update.get("accessibility", next_status.get("accessibility", False)));
            next_status["microphone"] = bool(update.get("microphone", next_status.get("microphone", False)));
            next_status["input_monitoring"] = bool(update.get("input_monitoring", next_status.get("input_monitoring", False)));
            grant = str(update.get("microphone_grant", next_status.get("microphone_grant", "not_asked")));
            charged_to = update.get("charged_to", next_status.get("charged_to", None));
            next_status["microphone_grant"] = grant;
            next_status["charged_to"] = charged_to;
            hint = microphone_permission_hint(grant, charged_to);
            if not hint { hint = "Lets OpenFlow hear what you say."; }
            next_status["microphone_hint"] = str(hint);
            permission_status = next_status;
            if bool(next_status["accessibility"]) and bool(next_status["microphone"]) { mark_entered(); }
        }

        async def refresh_permissions() {
            try { apply_permissions(dict(await invoke("get_status"))); } except Exception { }
            permission_loading = False;
        }

        async def grant_permission(command: str) {
            try { await invoke(command); } except Exception { }
            await refresh_permissions();
        }

        def apply_publik_status(update: dict) {
            remaining = update.get("starter_remaining_micros", None);
            grant_value = update.get("starter_granted_micros", 0);
            granted = int(grant_value) if grant_value is not None else 0;
            if granted <= 0 { granted = 50_000; }
            low = remaining is not None and int(remaining) > 0 and int(remaining) * 5 < granted;
            if str(update.get("exhausted_message", "")) { low_starter_dismissed = True; }
            if not low {
                low_starter_shown = False;
                low_starter_dismissed = False;
            } elif not low_starter_shown {
                low_starter_shown = True;
                low_starter_dismissed = False;
            }
            publik_status = dict(update);
        }

        async def dismiss_publik_notice() {
            low_starter_dismissed = True;
            try { apply_publik_status(dict(await invoke("publik_dismiss_notice"))); } except Exception { }
        }

        async def open_publik_notice() {
            current_notice = publik_exhausted_notice(publik_status);
            if not current_notice { current_notice = publik_low_starter_notice(publik_status); }
            try { await invoke("publik_open_url", {"url": str(current_notice.get("link_url", publik_status.get("dashboard_url", "https://publikhq.com/dashboard/api")))}); } except Exception { }
        }

        def update_model_status(event: dict) {
            asr_model = dict(event["detail"]);
        }

        async def handle_model_banner_action() {
            state = str(asr_model.get("state", "loading"));
            if state == "downloading" {
                try {
                    prompt_event = document.createEvent("CustomEvent");
                    prompt_event.initCustomEvent("openflow:show-model-prompt", False, False, None);
                    window.dispatchEvent(prompt_event);
                } except Exception { }
            } else {
                try { await invoke("download_asr_model"); } except Exception { }
            }
        }

        useEffect(lambda {
            window.addEventListener("openflow:model-state", update_model_status);
            async def subscribe() {
                try { if window.localStorage.getItem("openflow_onboarding_done") == "1" { entered = True; } } except Exception { }
                await refresh_permissions();
                try {
                    error = await invoke("get_last_error");
                    if error { last_error = dict(error); }
                } except Exception { }
                try { apply_publik_status(dict(await invoke("get_publik_status"))); } except Exception { }
                try {
                    await listen("openflow://permissions", lambda (event: dict) { apply_permissions(dict(event["payload"])); });
                } except Exception { }
                try {
                    await listen("openflow://error", lambda (event: dict) {
                        last_error = dict(event["payload"]);
                        error_dismissed = False;
                    });
                } except Exception { }
                try {
                    await listen("openflow://publik", lambda (event: dict) { apply_publik_status(dict(event["payload"])); });
                } except Exception { }
            }
            subscribe();
            return lambda { window.removeEventListener("openflow:model-state", update_model_status); };
        }, []);

        if permission_loading { return <main className="wf-loading" />; }
        if not entered and not (bool(permission_status.get("accessibility", False)) and bool(permission_status.get("microphone", False))) {
            return <main className="jac-onboarding">
                <section className="jac-onboarding-card">
                    <div className="jac-onboarding-brand"><h1>Set up OpenFlow</h1><span>LOCAL</span></div>
                    <p>Grant these permissions to OpenFlow in order. The required steps update as soon as macOS applies them.</p>
                    <article className={"jac-onboarding-step active" if not bool(permission_status.get("accessibility", False)) else "jac-onboarding-step done"}>
                        <span className="jac-onboarding-number">{"✓" if bool(permission_status.get("accessibility", False)) else "1"}</span>
                        <div><strong>Accessibility <small>· required</small></strong><p>Allows the global shortcut and types your words into the active app.</p></div>
                        {bool(permission_status.get("accessibility", False)) and <b>Granted</b>}
                        {not bool(permission_status.get("accessibility", False)) and <button onClick={lambda { grant_permission("request_accessibility"); }}>Grant</button>}
                    </article>
                    <article className={"jac-onboarding-step active" if bool(permission_status.get("accessibility", False)) and not bool(permission_status.get("microphone", False)) else "jac-onboarding-step done" if bool(permission_status.get("microphone", False)) else "jac-onboarding-step locked"}>
                        <span className="jac-onboarding-number">{"✓" if bool(permission_status.get("microphone", False)) else "2"}</span>
                        <div><strong>Microphone <small>· required</small></strong><p>{str(permission_status.get("microphone_hint", "Lets OpenFlow hear what you say."))}</p></div>
                        {bool(permission_status.get("microphone", False)) and <b>Granted</b>}
                        {not bool(permission_status.get("microphone", False)) and <button disabled={not bool(permission_status.get("accessibility", False))} onClick={lambda { grant_permission("request_microphone"); }}>Grant</button>}
                    </article>
                    <article className={"jac-onboarding-step active" if bool(permission_status.get("accessibility", False)) and bool(permission_status.get("microphone", False)) and not bool(permission_status.get("input_monitoring", False)) else "jac-onboarding-step done" if bool(permission_status.get("input_monitoring", False)) else "jac-onboarding-step locked"}>
                        <span className="jac-onboarding-number">{"✓" if bool(permission_status.get("input_monitoring", False)) else "3"}</span>
                        <div><strong>Input Monitoring <small>· optional</small></strong><p>Can improve key detection reliability. You can continue without it.</p></div>
                        {bool(permission_status.get("input_monitoring", False)) and <b>Granted</b>}
                        {not bool(permission_status.get("input_monitoring", False)) and <button disabled={not (bool(permission_status.get("accessibility", False)) and bool(permission_status.get("microphone", False)))} onClick={lambda { grant_permission("request_input_monitoring"); }}>Grant</button>}
                    </article>
                    <button className="jac-onboarding-enter" disabled={not (bool(permission_status.get("accessibility", False)) and bool(permission_status.get("microphone", False)))} onClick={lambda { mark_entered(); }}>Enter OpenFlow →</button>
                    {not (bool(permission_status.get("accessibility", False)) and bool(permission_status.get("microphone", False))) and <button className="jac-onboarding-skip" onClick={lambda { mark_entered(); }}>Skip for now — Settings and your local library remain available</button>}
                    <p className="jac-onboarding-footnote">If a permission stays grey after you change it in System Settings, toggle OpenFlow off and back on there. This screen updates when macOS reports the change.</p>
                </section>
            </main>;
        }
        accessibility_lapsed = entered and not bool(permission_status.get("accessibility", False));
        error_headline = "Accessibility permission needed" if accessibility_lapsed else str(last_error.get("headline", ""));
        error_detail = "OpenFlow can no longer type into other apps. Grant Accessibility again to restore dictation." if accessibility_lapsed else str(last_error.get("detail", ""));
        model_state = str(asr_model.get("state", "loading"));
        show_model_banner = not error_dismissed and (model_state == "missing" or model_state == "failed" or model_state == "downloading");
        model_headline = "Speech model not installed" if model_state == "missing" else "Speech model download failed" if model_state == "failed" else "Downloading speech model";
        model_detail = "Dictation needs a " + model_download_size_label() + " on-device speech model. Download it once to enable local dictation." if model_state == "missing" else model_failure_message(str(asr_model.get("message", ""))) if model_state == "failed" else model_megabytes(int(asr_model.get("received", 0))) + " of " + model_megabytes(int(asr_model.get("total", 0))) + " downloaded. Dictation works when it is done.";
        model_action_label = "Download speech model" if model_state == "missing" else "Try again" if model_state == "failed" else "Show progress";
        show_error = not error_dismissed and not show_model_banner and bool(error_headline);
        notice: dict = publik_exhausted_notice(publik_status);
        if not notice and not low_starter_dismissed { notice = publik_low_starter_notice(publik_status); }

        return <div className="jac-hub-frame">
            {show_model_banner and <div className="jac-model-banner">
                <span className="jac-model-banner-icon">{"↓" if model_state == "downloading" else "◉"}</span>
                <div><strong>{model_headline}</strong><span>{model_detail}</span></div>
                <button onClick={lambda { handle_model_banner_action(); }}>{model_action_label}</button>
                <button className="jac-model-banner-close" aria-label="Dismiss" onClick={lambda { error_dismissed = True; }}>×</button>
            </div>}
            {show_error and <div className="jac-hub-alert">
                <span className="jac-hub-alert-icon">!</span>
                <div className="jac-hub-alert-copy"><strong>{error_headline}</strong><span>{error_detail}</span></div>
                {accessibility_lapsed and <button onClick={lambda { grant_permission("request_accessibility"); }}>Grant Accessibility</button>}
                <button className="jac-hub-alert-close" aria-label="Dismiss" onClick={lambda { error_dismissed = True; }}>×</button>
            </div>}
            {notice and <div className="jac-publik-notice">
                <div><strong>{str(notice.get("headline", ""))}</strong><span>{str(notice.get("message", ""))}</span></div>
                <button onClick={lambda { open_publik_notice(); }}>{str(notice.get("link_label", "Learn more"))}</button>
                <button className="jac-publik-notice-close" aria-label="Dismiss" onClick={lambda { dismiss_publik_notice(); }}>×</button>
            </div>}
            <div className="jac-hub-shell">
            <CleanupBridge />
            <DictationBridge />
            <AudioPolicyBridge />
            <StatsBridge />
            <CalendarBridge />
            <AutoLearnBridge />
            <DiagnosticsBridge />
            <ModelBridge />
            <aside className="jac-hub-sidebar">
                <div className="jac-hub-brand">OpenFlow <span>LOCAL</span></div>
                <nav aria-label="Main navigation">
                    <button className={"jac-nav-item active" if page == "home" else "jac-nav-item"} onClick={lambda { page = "home"; }}><span>⌂</span>Home</button>
                    <button className={"jac-nav-item active" if page == "insights" else "jac-nav-item"} onClick={lambda { page = "insights"; }}><span>⌁</span>Insights</button>
                    <button className={"jac-nav-item active" if page == "meetings" else "jac-nav-item"} onClick={lambda { page = "meetings"; }}><span>▥</span>Meetings</button>
                    <button className={"jac-nav-item active" if page == "dictionary" else "jac-nav-item"} onClick={lambda { page = "dictionary"; }}><span>⌕</span>Dictionary</button>
                    <button className={"jac-nav-item active" if page == "snippets" else "jac-nav-item"} onClick={lambda { page = "snippets"; }}><span>▤</span>Snippets</button>
                    <button className={"jac-nav-item active" if page == "style" else "jac-nav-item"} onClick={lambda { page = "style"; }}><span>✦</span>Style</button>
                    <button className={"jac-nav-item active" if page == "transforms" else "jac-nav-item"} onClick={lambda { page = "transforms"; }}><span>⇄</span>Transforms</button>
                    <button className={"jac-nav-item active" if page == "notes" else "jac-nav-item"} onClick={lambda { page = "notes"; }}><span>✎</span>Notes</button>
                </nav>
                <nav className="jac-sidebar-bottom" aria-label="Help">
                    <button className={"jac-nav-item active" if page == "settings" else "jac-nav-item"} onClick={lambda { page = "settings"; }}><span>⚙</span>Settings</button>
                    <button className={"jac-nav-item active" if page == "help" else "jac-nav-item"} onClick={lambda { page = "help"; }}><span>?</span>Help</button>
                </nav>
                <div className="jac-sidebar-foot">Private by design<br /><small>Saved on this Mac</small></div>
            </aside>
            <div className="jac-hub-content">{<Home /> if page == "home" else <Insights /> if page == "insights" else <MeetingsHub /> if page == "meetings" else <Dictionary /> if page == "dictionary" else <Notes /> if page == "notes" else <SettingsPage /> if page == "settings" else <Help /> if page == "help" else <ComingSoon title="Snippets" description="Save reusable phrases and expand them by voice — signatures, addresses, boilerplate." icon="▤" /> if page == "snippets" else <ComingSoon title="Style" description="Tune OpenFlow's tone and formatting so cleaned-up text always sounds like you." icon="✦" /> if page == "style" else <ComingSoon title="Transforms" description="Turn a quick spoken thought into an email, a summary, or a to-do with one command." icon="⇄" />}</div>
            </div>
        </div>;
    }

    def:pub app() -> JsxElement {
        has window_label: str = "";
        async can with entry {
            try {
                window_label = str(getCurrentWindow().label);
            } except Exception {
                window_label = "openflow_bar";
            }
        }
        if not window_label { return <main className="wf-loading" />; }
        if window_label == "meeting_toast" {
            return <MeetingWindow />;
        }
        if window_label == "main" { return <HubApp />; }
        return <Overlay />;
    }
}

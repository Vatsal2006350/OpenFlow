"""Jac-owned speech-model status, prompt, retry, and progress UI."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .model_policy { model_download_allowed, model_download_file_label, model_download_size_label, model_failure_message, model_policy_fields, model_unavailable_action, model_progress, model_megabytes, reduce_model_state }

cl def:pub ModelBridge() -> JsxElement {
    has model: dict = {"state": "loading"};
    has prompt_open: bool = False;
    has just_installed: bool = False;
    model_ref: dict = {"fields": ["loading"]};

    def apply_model(next_model: dict) {
        current = str(next_model.get("state", "loading"));
        model = next_model;
        try {
            bridge_event = document.createEvent("CustomEvent");
            bridge_event.initCustomEvent("openflow:model-state", False, False, next_model);
            window.dispatchEvent(bridge_event);
        } except Exception { }
        if current == "missing" {
            prompt_open = True;
        }
        if current == "failed" {
            prompt_open = True;
        }
    }

    def model_from_fields(fields: list[str]) -> dict {
        if len(fields) == 0 { return {"state": "loading"}; }
        next_model: dict = {"state": fields[0]};
        if fields[0] == "ready" and len(fields) >= 3 {
            next_model["file"] = fields[1];
            next_model["bundled"] = fields[2] == "true";
        }
        if fields[0] == "downloading" and len(fields) >= 3 {
            next_model["received"] = int(fields[1]);
            next_model["total"] = int(fields[2]);
        }
        if fields[0] == "failed" and len(fields) >= 2 { next_model["message"] = fields[1]; }
        return next_model;
    }

    def apply_native_input(input: list[str]) {
        if len(input) == 0 { return; }
        if input[0] == "unavailable" { handle_unavailable(input); return; }
        previous = str(model_ref["fields"][0]);
        next_fields: list[str] = list(reduce_model_state(model_ref["fields"], input));
        model_ref["fields"] = next_fields;
        apply_model(model_from_fields(next_fields));
        if next_fields[0] == "ready" and previous == "downloading" {
            just_installed = True;
            prompt_open = True;
        }
    }

    async def handle_unavailable(fields: list[str]) {
        engine_ready = len(fields) >= 2 and fields[1] == "true";
        download_running = len(fields) >= 3 and fields[2] == "true";
        if engine_ready { return; }
        current_state = "downloading" if download_running else str(model_ref["fields"][0]);
        if download_running {
            apply_model({"state": "downloading", "received": int(model.get("received", 0)), "total": int(model.get("total", 0))});
        }
        action = model_unavailable_action(current_state);
        if action == "show_prompt" {
            prompt_open = True;
            try { await invoke("focus_model_prompt"); } except Exception { }
        }
        if action == "explain_download" {
            received = int(model.get("received", 0));
            total = int(model.get("total", 0));
            try {
                await invoke("report_jac_diagnostic", {
                    "code": "model_downloading",
                    "headline": "Speech model still downloading",
                    "detail": model_megabytes(received) + " of " + model_megabytes(total) + " downloaded. Dictation starts to work when the download is done."
                });
            } except Exception { }
        }
    }

    useEffect(lambda {
        async def subscribe() {
            try {
                await listen("openflow://asr-model/input", lambda (event: dict) {
                    apply_native_input(list(event["payload"]));
                });
                platform = str(await invoke("asr_platform"));
                await invoke("initialize_asr_model", {"fields": model_policy_fields(platform)});
            } except Exception { }
        }
        subscribe();
        return lambda { };
    }, []);

    async def start_download() {
        if not model_download_allowed(str(model.get("state", "loading"))) { return; }
        just_installed = False;
        try { await invoke("download_asr_model"); } except Exception { }
    }
    def close_prompt() {
        prompt_open = False;
        just_installed = False;
    }

    def reveal_prompt(_event: dict) { prompt_open = True; }

    useEffect(lambda {
        window.addEventListener("openflow:show-model-prompt", reveal_prompt);
        return lambda { window.removeEventListener("openflow:show-model-prompt", reveal_prompt); };
    }, []);

    state = str(model.get("state", "loading"));
    downloading = state == "downloading";
    failed = state == "failed";
    ready = state == "ready";
    received = int(model.get("received", 0));
    total = int(model.get("total", 0));
    pct = model_progress(received, total);
    show_success = ready and just_installed;

    if prompt_open {
        return <div className="jac-model-backdrop" role="presentation">
        <section className="jac-model-dialog" role="dialog" aria-modal="true" aria-labelledby="jac-model-title">
            <div className="jac-model-eyebrow">ON-DEVICE SPEECH</div>
            <h2 id="jac-model-title">{"Speech model installed" if show_success else "Download the speech model"}</h2>
            {show_success and <p>OpenFlow is ready. Hold your key and speak.</p>}
            {not show_success and <p>OpenFlow turns your voice into text with a model running on this computer. Download it once to enable local dictation.</p>}
            {not show_success and <small>{model_download_file_label()} · {model_download_size_label()} · from huggingface.co · one time</small>}
            {downloading and <div className="jac-model-progress">
                <div className="jac-model-track" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={pct}><span style={{"width": str(pct) + "%"}} /></div>
                <div className="jac-model-progress-meta"><span>{model_megabytes(received)} of {model_megabytes(total)}</span><span>{str(pct)}%</span></div>
                <small>The download continues if you hide this window.</small>
            </div>}
            {failed and <div className="jac-model-error">{model_failure_message(str(model.get("message", "")))}</div>}
            <div className="jac-model-actions">
                {ready and show_success and <button className="jac-primary-button" onClick={lambda { close_prompt(); }}>Done</button>}
                {downloading and <button className="jac-secondary-button" onClick={lambda { close_prompt(); }}>Hide</button>}
                {not ready and not downloading and <button className="jac-primary-button" disabled={state == "loading"} onClick={lambda { start_download(); }}>{"Try again" if failed else "Download speech model (" + model_download_size_label() + ")"}</button>}
                {not ready and not downloading and <button className="jac-secondary-button" onClick={lambda { close_prompt(); }}>Not now</button>}
            </div>
        </section>
        </div>;
    }
    return <span aria-hidden="true" className="jac-model-bridge" />;
}

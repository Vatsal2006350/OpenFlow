"""Jac client port of the OpenFlow dictation overlay."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from .overlay_policy { overlay_position }
cl import from react { useEffect }
cl import "./overlay.css";

cl def:pub Overlay() -> JsxElement {
    has state: str = "idle";
    has bars: list = [];
    has pointer_inside: bool = False;
    has hovered_action: str = "";
    has error_headline: str = "";
    has error_detail: str = "";
    useEffect(lambda {
        async def place_overlay() {
            try {
                facts: dict = dict(await invoke("get_overlay_layout_facts"));
                position = overlay_position(facts);
                await invoke("apply_overlay_position", position);
            } except Exception { }
        }
        async def subscribe() {
            try {
                await listen("openflow://overlay/reposition", lambda (_event: dict) { place_overlay(); });
            } except Exception { }
            await place_overlay();
        }
        subscribe();
    }, []);
    async can with entry {
        try {
            await listen("openflow://flowbar/state", lambda (event: dict) -> None {
                state = str(event["payload"]["state"]);
            });
            await listen("openflow://audio/waveform", lambda (event: dict) -> None {
                bars = list(event["payload"]["bars"]);
            });
            await listen("openflow://error", lambda (event: dict) -> None {
                error_headline = str(event["payload"]["headline"]);
                error_detail = str(event["payload"]["detail"]);
            });
            await listen("openflow://overlay/pointer", lambda (event: dict) -> None {
                pointer_now: bool = bool(event["payload"]["inside"]);
                pointer_inside = pointer_now;
                x: float = float(event["payload"]["x"]);
                hovered_action = "Dictate · hold ⌥ Option" if pointer_now and x >= 88 and x < 122 else "Hands-free · ⌥ Space" if pointer_now and x >= 123 and x < 157 else "Open meeting notes" if pointer_now and x >= 158 and x < 192 else "";
            });
        } except Exception {
            # The web preview has no Tauri event bridge.
        }
    }

    recording = state == "recording" or state == "locked";
    idle = state == "idle";
    processing = state == "transcribing";
    is_error = state == "error";
    status = "Cleaning up…" if processing else error_headline if is_error and error_headline else "Something's off" if is_error else "Discarded" if state == "cancelled" else "Done";
    hovered = pointer_inside and idle;
    indices = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    waveform_items: list[JsxElement] = [];
    for index in indices {
        bar_height = 3;
        if len(bars) > 0 {
            bar_height = 2 + float(bars[len(bars) - 1 - index % len(bars)]) * 10;
        }
        waveform_items.append(<span key={index} className={"wf-wave wf-wave-" + str(index)} style={{"height": str(bar_height) + "px"}} />);
    }

    async def command(name: str) {
        try {
            await invoke(name);
        } except Exception {
            # Ignore commands when rendered outside the desktop host.
        }
    }

    return <div className="wf-overlay-root">
        <div
            className={"flowbar-pill" + (" flowbar-pill-idle" if idle else "") + (" flowbar-pill-hovered" if hovered else "")}
            aria-label={"OpenFlow " + state + (": " + error_detail if is_error else "")}
            title={error_detail if is_error else ""}
        >
            {idle and <>
                <button className="flowbar-compact" aria-label="OpenFlow actions" title="OpenFlow actions" />
                <div className="flowbar-idle-actions">
                    <button data-flow-action="Dictate · hold ⌥ Option" className={"flowbar-action" + (" flowbar-action-hovered" if hovered_action == "Dictate · hold ⌥ Option" else "")} aria-label="Dictate · hold ⌥ Option" title="Dictate · hold ⌥ Option" onClick={lambda { command("toggle_hands_free_dictation"); }}>
                        <span className="wf-mic-icon">●</span><span className="flowbar-tooltip">Dictate · hold ⌥ Option</span>
                    </button>
                    <button data-flow-action="Hands-free · ⌥ Space" className={"flowbar-action flowbar-action-featured" + (" flowbar-action-hovered" if hovered_action == "Hands-free · ⌥ Space" else "")} aria-label="Hands-free · ⌥ Space" title="Hands-free · ⌥ Space" onClick={lambda { command("toggle_hands_free_dictation"); }}>
                        <span className="wf-handsfree-icon">●</span><span className="flowbar-tooltip">Hands-free · ⌥ Space</span>
                    </button>
                    <button data-flow-action="Open meeting notes" className={"flowbar-action" + (" flowbar-action-hovered" if hovered_action == "Open meeting notes" else "")} aria-label="Open meeting notes" title="Open meeting notes" onClick={lambda { command("open_meetings"); }}>
                        <span className="wf-note-icon">▤</span><span className="flowbar-tooltip">Open meeting notes</span>
                    </button>
                </div>
            </>}
            {recording and <>
                <button className="flowbar-stop-cancel" aria-label="Cancel dictation" title="Cancel (Esc)" onClick={lambda { command("cancel_dictation"); }}>×</button>
                <div className="wf-waveform" aria-label="Recording">
                    {waveform_items}
                </div>
                <button className="flowbar-stop-confirm" aria-label="Stop dictation" title="Stop" onClick={lambda { command("stop_dictation"); }}>✓</button>
            </>}
            {not idle and not recording and <span className="flowbar-status">{status}</span>}
        </div>
    </div>;
}

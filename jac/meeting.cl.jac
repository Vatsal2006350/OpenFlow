"""Jac client for the live meeting note window and compact reminder."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .meeting_policy { merge_rolling_transcript }
cl import from .meeting_workflow { process_and_save_meeting }
cl import "./meeting.css";

cl def:pub MeetingWindow() -> JsxElement {
    has phase: str = "idle";
    has meeting: dict | None = None;
    has compact: bool = False;
    has title: str = "";
    has transcript: str = "";
    has thoughts: str = "";
    has secs: int = 0;
    has tab: str = "transcript";
    has error: str = "";
    has pending: bool = False;
    has saved: dict | None = None;
    phase_ref: dict[str, str] = {"current": "idle"};

    async can with entry {
        try {
            await listen("openflow://meeting/starting", lambda (event: dict) {
                if phase_ref["current"] == "recording" or phase_ref["current"] == "processing" { return; }
                data: dict = event["payload"];
                meeting = data;
                title = str(data.get("title", "Untitled meeting"));
                transcript = "";
                thoughts = "";
                saved = None;
                secs = 0;
                error = "";
                compact = True;
                phase = "reminder";
                phase_ref["current"] = "reminder";
            });
            await listen("openflow://meeting/recording", lambda (event: dict) {
                data: dict = event["payload"];
                title = str(data.get("title", "Untitled meeting"));
                transcript = "";
                thoughts = "";
                secs = 0;
                saved = None;
                tab = "transcript";
                compact = False;
                pending = False;
                phase = "recording";
                phase_ref["current"] = "recording";
            });
            await listen("openflow://meeting/live-transcript", lambda (event: dict) {
                if phase_ref["current"] != "recording" { return; }
                data: dict = event["payload"];
                rolling_text = str(data.get("rolling_text", ""));
                transcript = merge_rolling_transcript(transcript, rolling_text);
                secs = int(data.get("seconds", 0));
            });
            await listen("openflow://meeting/processing", lambda (event: dict) {
                compact = False;
                tab = "summary";
                pending = False;
                phase = "processing";
                phase_ref["current"] = "processing";
            });
            await listen("openflow://meeting/saved", lambda (event: dict) {
                data: dict = event["payload"];
                saved = data;
                title = str(data.get("title", "Untitled meeting"));
                transcript = str(data.get("transcript", ""));
                thoughts = str(data.get("thoughts", ""));
                compact = False;
                tab = "summary";
                pending = False;
                error = "";
                phase = "saved";
                phase_ref["current"] = "saved";
            });
            await listen("openflow://meeting/error", lambda (event: dict) {
                error = str(event["payload"]);
                pending = False;
                compact = False;
                phase = "error";
                phase_ref["current"] = "error";
            });
            await listen("openflow://meeting/cancelled", lambda (event: dict) {
                phase = "idle";
                phase_ref["current"] = "idle";
                meeting = None;
                pending = False;
                hide_note();
            });
        } except Exception {
            # Native event subscriptions are unavailable in a plain browser.
        }
        if phase_ref["current"] == "idle" { hide_note(); }
    }

    active = phase == "recording";
    busy = phase == "processing" or pending;
    is_idle = phase == "idle";
    fmt = str(int(secs / 60)).zfill(2) + ":" + str(secs % 60).zfill(2);

    useEffect(lambda {
        if not active { return; }
        timer = setInterval(lambda { secs = secs + 1; }, 1000);
        return lambda { clearInterval(timer); };
    }, [active]);

    async def hide_note() {
        try { await invoke("hide_meeting_note"); } except Exception {}
    }

    async def start_recording() {
        if busy or active { return; }
        pending = True;
        error = "";
        try {
            await invoke("start_meeting_recording", {"title": title.strip() or "Untitled meeting"});
            if meeting and meeting.get("join_url") {
                await invoke("open_meeting_url", {"url": meeting["join_url"]});
            }
        } except Exception as err {
            error = str(err);
        }
        pending = False;
    }

    async def finish_recording() {
        if not active or pending { return; }
        pending = True;
        error = "";
        try {
            raw_transcript: str = str(await invoke("stop_meeting_recording"));
            transcript = raw_transcript;
            result: dict = dict(await process_and_save_meeting(title, raw_transcript, thoughts));
            if result.get("error", "") {
                error = str(result["error"]);
                phase = "error";
                phase_ref["current"] = "error";
            } elif result.get("meeting") {
                saved = dict(result["meeting"]);
                phase = "saved";
                phase_ref["current"] = "saved";
                tab = "summary";
            }
        } except Exception as err {
            error = str(err);
            phase = "error";
            phase_ref["current"] = "error";
        }
        pending = False;
    }

    async def close_note() {
        if busy { return; }
        if active {
            await invoke("compact_meeting_note");
            compact = True;
            return;
        }
        await hide_note();
        phase = "idle";
        phase_ref["current"] = "idle";
        meeting = None;
    }

    async def back_to_meetings() {
        if active { await close_note(); return; }
        if busy { return; }
        await invoke("open_meetings");
        await close_note();
    }

    async def discard_recording() {
        if not busy { await invoke("cancel_meeting_recording"); }
    }

    async def reopen_note() {
        await invoke("expand_meeting_note");
        compact = False;
    }

    summary_status = "";
    if saved {
        summary_status = str(saved.get("summary_status", "generated"));
        words = len(str(saved.get("transcript", "")).split());
        if words < 25 or str(saved.get("summary", "")).strip() == str(saved.get("transcript", "")).strip() {
            summary_status = "insufficient";
        }
    }

    summary_blocks: list[JsxElement] = [];
    if saved {
        summary_text = str(saved.get("summary", ""));
        lines: list[str] = summary_text.split("\n");
        for index in range(len(lines)) {
            clean: str = str(lines[index]).strip();
            if clean.startswith("### ") or clean.startswith("## ") or clean.startswith("# ") {
                clean = clean.lstrip("# ").strip();
            }
            clean = clean.replace("**", "").strip();
            if not clean {
                summary_blocks.append(<div className="summary-gap" key={index} />);
            } elif clean.lower().rstrip(":") in ["summary", "decisions", "action items", "next steps"] {
                summary_blocks.append(<h3 key={index}>{clean.rstrip(":")}</h3>);
            } elif clean.startswith("- ") or clean.startswith("• ") {
                summary_blocks.append(<div className="summary-bullet" key={index}><span>•</span><span>{clean[2:]}</span></div>);
            } else {
                summary_blocks.append(<p key={index}>{clean}</p>);
            }
        }
    }

    if is_idle { return <></>; }

    if compact and (active or phase == "reminder") {
        return <main className="reminder">
            <header>
                <span className={"status-dot recording" if active else "status-dot"} />
                <span>{"RECORDING LOCALLY" if active else "MEETING STARTING"}</span>
                <button className="icon-button" aria-label={"Discard recording" if active else "Dismiss reminder"} onClick={lambda { discard_recording() if active else close_note(); }}>×</button>
            </header>
            <h2>{title or "Untitled meeting"}</h2>
            <p>{"Mic + Mac audio · " + fmt if active else str(meeting.get("account_email", "Calendar event")) + " · starts when you join" if meeting else "Calendar event · starts when you join"}</p>
            {error and <p className="error" role="alert">{error}</p>}
            <footer>
                {active and <>
                    <button disabled={busy} onClick={lambda { reopen_note(); }}>Open notes</button>
                    <button className="primary" disabled={busy} onClick={lambda { finish_recording(); }}>Finish</button>
                </>}
                {not active and <>
                    <button disabled={busy} onClick={lambda { close_note(); }}>Not now</button>
                    <button className="primary" disabled={busy} onClick={lambda { start_recording(); }}>{"Starting…" if pending else "Join meeting" if meeting and meeting.get("join_url") else "Start notes"}</button>
                </>}
            </footer>
        </main>;
    }

    thoughts_panel = <textarea aria-label="Meeting thoughts" value={thoughts} readOnly={saved or busy} onChange={lambda (event: dict) { thoughts = str(event["target"]["value"]); }} placeholder="Capture your thoughts while the transcript runs…" />;
    transcript_panel = <>
        <div className="transcript-status"><span>◷ {fmt}</span><span>{"Mic + Mac audio · on-device" if active else "Transcript"}</span></div>
        {transcript and <p className="transcript-copy">{transcript}</p>}
        {not transcript and <div className="empty-state">{"Listening… Your transcript will appear as you speak." if active else "No speech was recognized."}</div>}
    </>;

    summary_panel = <></>;
    if phase == "processing" {
        summary_panel = <div aria-live="polite">
            <div className="notice">Saving your transcript and creating the summary on this Mac…</div>
            <div className="skeleton"><i/><i/><i/><i/><i/><i/></div>
        </div>;
    } elif saved {
        summary_panel = <>
            <div className={"notice neutral" if summary_status != "generated" else "notice"}>{"✓ Summary ready · generated on this Mac" if summary_status == "generated" else "Short recording · transcript saved" if summary_status == "insufficient" else "Transcript saved · summary unavailable"}</div>
            {summary_status == "insufficient" and <p className="muted">There isn’t enough meeting content for a useful summary yet. Your recording is available in the Transcript tab.</p>}
            {summary_status != "insufficient" and <div className="summary-copy">{summary_blocks}</div>}
        </>;
    } else {
        summary_panel = <div className="empty-state">{"The recording has ended. You can review any live text in the Transcript tab." if phase == "error" else "Your summary will be generated automatically when you finish."}</div>;
    }

    current_panel = thoughts_panel if tab == "thoughts" else transcript_panel if tab == "transcript" else summary_panel;

    return <main className="note">
        <header className="note-header">
            <button className="icon-button back" aria-label="Back to meetings" disabled={busy} onClick={lambda { back_to_meetings(); }}>‹</button>
            <span className="brand">OpenFlow <span>· Local note</span></span>
            <span className={"note-status live" if active else "note-status"}>{"● LIVE  " + fmt if active else "SAVING…" if phase == "processing" else "SAVED ON THIS DEVICE" if saved else "RECORDING ENDED"}</span>
            <button className="icon-button" aria-label={"Minimize recording" if active else "Close note"} disabled={busy} onClick={lambda { close_note(); }}>×</button>
        </header>
        <div className="note-body">
            <input className="note-title" aria-label="Meeting title" value={title} onChange={lambda (event: dict) { title = str(event["target"]["value"]); }} placeholder="Untitled meeting" disabled={busy or bool(saved)} />
            <div className="note-meta"><span>Local recording</span><span>On this Mac</span></div>
            <nav aria-label="Note tabs">
                <button aria-selected={tab == "thoughts"} onClick={lambda { tab = "thoughts"; }}>My thoughts</button>
                <button aria-selected={tab == "transcript"} onClick={lambda { tab = "transcript"; }}>Transcript</button>
                <button aria-selected={tab == "summary"} onClick={lambda { tab = "summary"; }}>✦ Summary</button>
            </nav>
            <section className="note-content">
                {current_panel}
                {error and <div className="error" role="alert">{error}</div>}
            </section>
            <footer className="note-footer">
                {active and <>
                    <p>Mic and Mac audio stay on this device. Please tell others before recording.</p>
                    <div><button className="quiet" disabled={busy} onClick={lambda { discard_recording(); }}>Discard</button><button className="primary" disabled={busy} onClick={lambda { finish_recording(); }}>■ Finish &amp; summarize</button></div>
                </>}
                {not active and <div><span className="muted">{"You can keep using your other apps." if busy else "Your notes stay on this Mac."}</span><button className="primary" disabled={busy} onClick={lambda { back_to_meetings(); }}>Back to meetings</button></div>}
            </footer>
        </div>
    </main>;
}

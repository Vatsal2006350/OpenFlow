"""Jac client for OpenFlow's local meeting library and calendar."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .meeting_workflow { process_and_save_meeting }
cl import "./meetings-hub.css";

cl def:pub MeetingsHub() -> JsxElement {
    has meetings: list[dict] = [];
    has archive: list[dict] = [];
    has selected: dict | None = None;
    has query: str = "";
    has title: str = "";
    has transcript: str = "";
    has composing: bool = False;
    has error: str = "";
    has recording: bool = False;
    has record_secs: int = 0;
    has processing: bool = False;
    has accounts: list[dict] = [];
    has events: list[dict] = [];
    has calendar_status: str = "";
    has summary_copy: str = "";

    def search_rank(hit: dict) -> tuple[int, int] {
        meeting: dict = hit["meeting"];
        return (-int(hit["score"]), -int(meeting.get("created_at", 0)));
    }

    def recent_rank(meeting: dict) -> int {
        return -int(meeting.get("created_at", 0));
    }

    def rank_meetings(rows: list[dict], search: str, limit: int = 12) -> list[dict] {
        rows.sort(key=recent_rank);
        terms: list[str] = search.lower().split();
        if len(terms) == 0 { return rows[:limit]; }
        hits: list[dict] = [];
        for meeting in rows {
            content = " ".join([
                str(meeting.get("title", "")),
                str(meeting.get("summary", "")),
                str(meeting.get("transcript", ""))
            ]).lower();
            score = 0;
            for term in terms { if term in content { score += 1; } }
            if score > 0 { hits.append({"score": score, "meeting": meeting}); }
        }
        hits.sort(key=search_rank);
        selected_rows: list[dict] = [];
        for hit in hits[:limit] { selected_rows.append(hit["meeting"]); }
        return selected_rows;
    }

    async def refresh_meetings(search: str = "", force: bool = False) {
        if len(archive) > 0 and not force {
            meetings = rank_meetings(archive, search);
            return;
        }
        try {
            rows: list[dict] = list(await invoke("list_meetings"));
            archive = rows;
            meetings = rank_meetings(rows, search);
        } except Exception {
            meetings = [];
        }
    }

    async def refresh_calendar() {
        try {
            account_rows: list[dict] = list(await invoke("get_google_calendar_accounts"));
            event_rows: list[dict] = list(await invoke("get_upcoming_calendar_events"));
            accounts = account_rows;
            events = event_rows;
        } except Exception {
            # Calendar data is optional; the local meeting archive stays available.
        }
    }

    async can with entry {
        await refresh_meetings();
        await refresh_calendar();
        try {
            await listen("openflow://meeting/recording", lambda (event: dict) {
                data: dict = event["payload"];
                recording = True;
                processing = False;
                record_secs = 0;
                title = str(data.get("title", "Untitled meeting"));
                error = "";
            });
            await listen("openflow://meeting/live-transcript", lambda (event: dict) {
                data: dict = event["payload"];
                record_secs = int(data.get("seconds", record_secs));
            });
            await listen("openflow://meeting/processing", lambda (event: dict) {
                recording = False;
                processing = True;
            });
            await listen("openflow://meeting/saved", lambda (event: dict) {
                item: dict = event["payload"];
                recording = False;
                processing = False;
                selected = item;
                refresh_meetings(query, True);
            });
            await listen("openflow://meeting/cancelled", lambda (event: dict) {
                recording = False;
                processing = False;
            });
            await listen("openflow://meeting/error", lambda (event: dict) {
                recording = False;
                processing = False;
                error = str(event["payload"]);
            });
            await listen("openflow://calendar/status", lambda (event: dict) {
                data: dict = event["payload"];
                calendar_state = str(data.get("state", ""));
                if calendar_state == "connected" {
                    calendar_status = str(data.get("email", ""));
                } else {
                    calendar_status = str(data.get("message", "Calendar connection failed"));
                }
                refresh_calendar();
            });
        } except Exception {
            # Web previews have no native event bridge.
        }
    }

    useEffect(lambda {
        timer = setInterval(lambda { refresh_calendar(); }, 30000);
        return lambda { clearInterval(timer); };
    }, []);
    useEffect(lambda {
        timer = setTimeout(lambda { refresh_meetings(query); }, 180);
        return lambda { clearTimeout(timer); };
    }, [query]);
    useEffect(lambda {
        if not recording { return; }
        timer = setInterval(lambda { record_secs = record_secs + 1; }, 1000);
        return lambda { clearInterval(timer); };
    }, [recording]);

    async def connect_calendar() {
        calendar_status = "Opening Google sign-in…";
        try { await invoke("connect_google_calendar"); }
        except Exception as err { calendar_status = str(err); }
    }

    async def start_recording(requested_title: str = "") {
        error = "";
        meeting_title = (requested_title or title).strip() or "Untitled meeting";
        try {
            await invoke("start_meeting_recording", {"title": meeting_title});
            title = meeting_title;
            record_secs = 0;
            recording = True;
            selected = None;
            composing = False;
        } except Exception as err { error = str(err); }
    }

    async def finish_recording() {
        error = "";
        recording = False;
        processing = True;
        try {
            saved_title = title;
            raw_transcript: str = str(await invoke("stop_meeting_recording"));
            result: dict = dict(await process_and_save_meeting(saved_title, raw_transcript));
            if result.get("error", "") {
                error = str(result["error"]);
            } else {
                title = "";
                composing = False;
                rows: list[dict] = list(await invoke("list_meetings"));
                archive = rows;
                meetings = rank_meetings(rows, query);
                for item in rows {
                    candidate_title = str(item.get("title", ""));
                    if candidate_title == saved_title {
                        selected = item;
                        break;
                    }
                }
            }
        } except Exception as err { error = str(err); }
        processing = False;
    }

    async def cancel_recording() {
        try { await invoke("cancel_meeting_recording"); } except Exception {}
        recording = False;
        composing = False;
    }

    async def save_transcript() {
        if processing { return; }
        error = "";
        processing = True;
        try {
            saved_title = title.strip() or "Untitled meeting";
            result: dict = dict(await process_and_save_meeting(saved_title, transcript));
            if result.get("error", "") {
                error = str(result["error"]);
            } else {
                title = "";
                transcript = "";
                composing = False;
                rows: list[dict] = list(await invoke("list_meetings"));
                archive = rows;
                meetings = rank_meetings(rows, query);
                for item in rows {
                    candidate_title = str(item.get("title", ""));
                    if candidate_title == saved_title {
                        selected = item;
                        break;
                    }
                }
            }
        } except Exception as err { error = str(err); }
        processing = False;
    }

    formatted_secs = str(int(record_secs / 60)).zfill(2) + ":" + str(record_secs % 60).zfill(2);
    account_names: list[str] = [];
    for account in accounts { account_names.append(str(account.get("email", ""))); }
    calendar_line = calendar_status or (" · ".join(account_names) if len(accounts) > 0 else "Connect up to two accounts to get a popup when a meeting begins. Calendar events are cached locally; access is read-only.");

    meeting_rows: list[JsxElement] = [];
    for item in meetings {
        summary: str = str(item.get("summary", ""));
        first_line = summary.split("\n")[0].replace("• ", "");
        word_count = len(str(item.get("transcript", "")).split());
        selected_row = False;
        if selected {
            selected_id = str(selected.get("id", ""));
            item_id = str(item.get("id", ""));
            selected_row = selected_id == item_id;
        }
        meeting_rows.append(<button className={"meeting-row selected" if selected_row else "meeting-row"} key={str(item.get("id", ""))} onClick={lambda { selected = item; composing = False; }}>
            <span className="meeting-row-title">{str(item.get("title", "Untitled meeting"))}</span>
            <span className="meeting-row-meta">{word_count} words · saved on this Mac</span>
            <span className="meeting-row-summary">{first_line}</span>
        </button>);
    }

    calendar_rows: list[JsxElement] = [];
    for event in events[:4] {
        start: str = str(event.get("start", ""));
        shown_time = start[:16].replace("T", " ");
        calendar_rows.append(<div className="calendar-row" key={str(event.get("account_email", "")) + str(event.get("id", ""))}>
            <span>{str(event.get("title", "Meeting"))}</span><time>{shown_time}</time>
        </div>);
    }

    details = <div className="meeting-empty"><span className="meeting-empty-icon">▤</span><h2>Your meeting notes, together</h2><p>Select a meeting to read its summary and transcript, or import a transcript to start your local library.</p></div>;
    if selected {
        summary_status = str(selected.get("summary_status", "generated"));
        if summary_status == "insufficient" or (not summary_status and len(str(selected.get("transcript", "")).split()) < 25) {
            summary_copy = "This recording is too short for a useful summary. The transcript is saved below.";
        } else { summary_copy = str(selected.get("summary", "")); }
        details = <article className="meeting-detail">
            <div className="meeting-detail-meta">Saved on this device</div>
            <h2>{str(selected.get("title", "Untitled meeting"))}</h2>
            <section><h3>Summary</h3><p>{summary_copy}</p></section>
            <section className="detail-transcript"><h3>Transcript</h3><p>{str(selected.get("transcript", ""))}</p></section>
        </article>;
    } elif composing {
        details = <section className="transcript-import">
            <h2>Add meeting notes</h2>
            <p>Paste a transcript to save it locally and create a quick summary. No audio or text is uploaded.</p>
            <input aria-label="Meeting title" value={title} onChange={lambda (event: dict) { title = str(event["target"]["value"]); }} placeholder="Meeting title" />
            <textarea aria-label="Transcript" value={transcript} onChange={lambda (event: dict) { transcript = str(event["target"]["value"]); }} placeholder="Paste transcript…" rows={12} />
            {error and <div className="hub-error">{error}</div>}
            <div className="import-actions"><button className="button-quiet" onClick={lambda { composing = False; }}>Cancel</button><button className="button-primary" disabled={not transcript.strip() or processing} onClick={lambda { save_transcript(); }}>{"Saving…" if processing else "Save locally"}</button></div>
        </section>;
    }

    return <main className="meetings-hub">
        <header className="meetings-page-heading">
            <div><div className="meetings-eyebrow">Private library</div><h1>Meetings</h1><p>Transcripts, summaries, and notes stay on this device.</p></div>
            <div className="meetings-heading-actions"><button className="button-quiet" onClick={lambda { selected = None; composing = True; }}>＋ Import transcript</button><button className="button-primary" disabled={recording or processing} onClick={lambda { start_recording(); }}>{"Transcribing locally…" if processing else "● Record meeting"}</button></div>
        </header>

        <section className="hub-card calendar-connect">
            <div><strong>Google Calendar</strong><span className="calendar-count"> · {len(accounts)} account{ "s" if len(accounts) != 1 else ""} connected</span><p>{calendar_line}</p></div>
            {len(accounts) < 2 and <button className="button-quiet" onClick={lambda { connect_calendar(); }}>{"Connect second account" if len(accounts) else "Connect account"}</button>}
        </section>
        {len(calendar_rows) > 0 and <section className="hub-card upcoming"><h2>Upcoming meetings</h2>{calendar_rows}</section>}
        {(recording or processing or error) and <section className="hub-card recording-status">
            <div><strong className={"recording-label" if recording else ""}>{"● Recording · " + formatted_secs if recording else "Transcribing locally…" if processing else error}</strong><p>{error and "Check microphone access and local Whisper setup, then try again." or "Your live transcript is open in the OpenFlow note window."}</p></div>
            {recording and <div className="recording-actions"><button className="button-quiet" onClick={lambda { cancel_recording(); }}>Discard</button><button className="button-primary" onClick={lambda { finish_recording(); }}>Finish &amp; summarize</button></div>}
        </section>}

        <div className="meetings-library">
            <section className="hub-card meeting-list">
                <label className="meeting-search"><span>⌕</span><input aria-label="Search meetings" placeholder="Search meetings" value={query} onChange={lambda (event: dict) { query = str(event["target"]["value"]); }} /></label>
                {meeting_rows if len(meeting_rows) else <div className="meeting-list-empty">{"No meetings match your search." if query else "Your local meeting library is ready."}</div>}
            </section>
            <section className="hub-card meeting-detail-panel">{details}</section>
        </div>
    </main>;
}

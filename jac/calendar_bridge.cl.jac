"""Apply Jac calendar policy to event snapshots from the native sync adapter."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .calendar_policy { calendar_query_window, normalize_google_calendar_event, select_upcoming_calendar_events, calendar_reminder_due, calendar_reminder_key }

cl def:pub CalendarBridge() -> JsxElement {
    has announced: dict[str, bool] = {};
    has syncing: bool = False;

    async def process_events(events: list[dict]) {
        now_ms = int(Date.now());
        rows: list[dict] = select_upcoming_calendar_events(events, now_ms);
        try { await invoke("save_upcoming_calendar_events", {"events": rows}); } except Exception { }
        for row in rows {
            if calendar_reminder_due(row, now_ms) {
                key = calendar_reminder_key(row);
                if not bool(announced.get(key, False)) {
                    announced[key] = True;
                    try { await invoke("display_calendar_reminder", {"event": row}); } except Exception { }
                }
            }
        }
        if len(announced) > 1000 { announced = {}; }
    }

    async def sync_calendar() {
        if syncing { return; }
        syncing = True;
        try {
            window: dict = calendar_query_window(int(Date.now()));
            feeds: list[dict] = list(await invoke("sync_google_calendar", {
                "timeMin": str(window["time_min"]),
                "timeMax": str(window["time_max"])
            }));
            events: list[dict] = [];
            for feed in feeds {
                account_email = str(feed.get("account_email", ""));
                items: list[dict] = list(feed.get("items", []));
                for item in items {
                    event: dict = normalize_google_calendar_event(item, account_email);
                    if event { events.append(event); }
                }
            }
            await process_events(events);
        } except Exception { }
        syncing = False;
    }

    useEffect(lambda {
        async def subscribe() {
            try {
                await listen("openflow://calendar/status", lambda (event: dict) {
                    if str(event.get("payload", {}).get("state", "")) == "connected" { sync_calendar(); }
                });
            }
            except Exception { }
        }
        subscribe();
        sync_calendar();
        timer = setInterval(lambda { sync_calendar(); }, 30_000);
        return lambda { clearInterval(timer); };
    }, []);
    return <span aria-hidden="true" className="jac-calendar-bridge" />;
}

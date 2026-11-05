"""Jac client for the local dictation dashboard."""

cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .stats { summarize_stats, recent_history }

cl def:pub Home() -> JsxElement {
    has stats: list[str] = ["0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0"];
    has history: list[dict] = [];
    has query: str = "";
    has loading: bool = True;

    async def refresh() {
        try {
            loaded_records: list[dict] = list(await invoke("get_stats_records"));
            environment: list[int] = list(await invoke("get_stats_environment"));
            stats = list(summarize_stats(loaded_records, environment[0], environment[1]));
            history = list(recent_history(loaded_records));
            loading = False;
        } except Exception {
            loading = False;
        }
    }

    useEffect(lambda {
        refresh();
        timer = setInterval(lambda { refresh(); }, 8000);
        return lambda { clearInterval(timer); };
    }, []);

    filtered: list[dict] = [];
    query_lower = query.strip().lower();
    for item in history {
        text_lower = str(item.get("text", "")).lower();
        if not query_lower or text_lower.find(query_lower) >= 0 {
            filtered.append(item);
        }
    }
    history_rows: list[JsxElement] = [];
    for item in filtered {
        app_name = str(item.get("app", ""));
        history_rows.append(<article className="jac-history-row" key={str(item.get("ts_unix", ""))}>
            <time>Local dictation</time>
            <div><p>{str(item.get("text", ""))}</p>{app_name and <small>{app_name}</small>}</div>
        </article>);
    }

    return <section className="jac-home">
        <header className="jac-home-heading">
            <div className="jac-home-eyebrow">Your workspace</div>
            <h1>Good to have you back.</h1>
            <p>{stats[4] + " words dictated today. Your recent activity is below." if int(stats[4]) > 0 else "Your words stay yours. Start a dictation whenever you’re ready."}</p>
        </header>
        <div className="jac-home-grid">
            <div className="jac-home-main">
                <section className="jac-home-hero">
                    <div className="jac-home-eyebrow">Voice, wherever you write</div>
                    <h2>Speak naturally. Your words stay on this device.</h2>
                    <p>Hold your push-to-talk key to dictate, or use your hands-free shortcut. Release or press again to insert your words at the cursor.</p>
                    <div className="jac-home-shortcuts"><span>Push-to-talk</span><span>hold to speak</span><i>·</i><span>Hands-free</span><span>press to toggle</span></div>
                </section>
                <section className="jac-home-card">
                    <div className="jac-home-card-heading"><strong>Recent dictations</strong><input aria-label="Search dictations" placeholder="Search history" value={query} onChange={lambda (event: dict) { query = str(event["target"]["value"]); }} /></div>
                    {loading and <div className="jac-home-empty">Loading your local history…</div>}
                    {not loading and len(history) == 0 and <div className="jac-home-empty">Your dictations will show up here. Hold your key and start speaking.</div>}
                    {not loading and len(history) > 0 and len(filtered) == 0 and <div className="jac-home-empty">No dictations match “{query}”.</div>}
                    {history_rows}
                </section>
            </div>
            <section className="jac-home-card jac-stats-card">
                <div className="jac-home-card-heading"><strong>On this Mac</strong><span>Private</span></div>
                <div className="jac-total-words">{stats[0]}<small>total words</small></div>
                <div className="jac-stat-pair"><div><b>{stats[2]}</b><small>avg WPM</small></div><div><b>{stats[5]}</b><small>day streak</small></div></div>
                <p className="jac-stats-note">{stats[3] + " WPM best · saved you " + str(int(int(stats[7]) / 60)) + " min vs typing · " + stats[1] + " dictations saved locally."}</p>
            </section>
        </div>
    </section>;
}

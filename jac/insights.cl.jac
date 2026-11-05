"""Jac usage insights built from local dictation session records."""

cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .stats { summarize_stats }

cl def:pub Insights() -> JsxElement {
    has stats: list[str] = ["0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0", "0"];
    has tab: str = "usage";
    async def refresh() {
        try {
            records: list[list[int]] = list(await invoke("get_stats_records"));
            environment: list[int] = list(await invoke("get_stats_environment"));
            stats = list(summarize_stats(records, environment[0], environment[1]));
        } except Exception {}
    }
    useEffect(lambda {
        refresh();
        timer = setInterval(lambda { refresh(); }, 10000);
        return lambda { clearInterval(timer); };
    }, []);

    max_day = 1;
    for index in range(8, 15) { max_day = max(max_day, int(stats[index])); }
    activity_bars: list[JsxElement] = [];
    day_labels = ["6 days ago", "5 days ago", "4 days ago", "3 days ago", "2 days ago", "Yesterday", "Today"];
    for offset in range(7) {
        count = int(stats[8 + offset]);
        height = 4 if count == 0 else max(8, int(count * 100 / max_day));
        activity_bars.append(<div className="jac-activity-column" key={offset}>
            <div className="jac-activity-value">{str(count)}</div>
            <div className="jac-activity-track"><div className="jac-activity-fill" style={{"height": str(height) + "%"}} /></div>
            <small>{day_labels[offset]}</small>
        </div>);
    }
    streak_dots: list[JsxElement] = [];
    for index in range(7) {
        value = stats[8 + index];
        streak_dots.append(<span key={index} className={"active" if int(value) > 0 else ""} title={value + " words"} />);
    }
    streak_label = "day" if stats[5] == "1" else "days";

    if tab == "voice" {
        return <section className="jac-insights">
            <div className="jac-home-eyebrow">Patterns in your dictation</div><h1>Insights</h1>
            <nav className="jac-insights-tabs"><button onClick={lambda { tab = "usage"; }}>Your Usage</button><button className="active">Your Voice</button></nav>
            <div className="jac-insights-card jac-insights-empty"><h2>Your Voice</h2><p>Tone, pace, and filler-word insights will appear here as you dictate. Your recordings stay on this device.</p></div>
        </section>;
    }
    return <section className="jac-insights">
        <div className="jac-home-eyebrow">Your practice, at a glance</div><h1>Insights</h1>
        <nav className="jac-insights-tabs"><button className="active">Your Usage</button><button onClick={lambda { tab = "voice"; }}>Your Voice</button></nav>
        <div className="jac-insights-grid">
            <article className="jac-insights-card"><div className="jac-insights-label">Average speaking speed</div><strong>{stats[2]}<small> words per minute</small></strong><p>Today: {stats[6]} WPM · measured from your local dictation sessions.</p></article>
            <article className="jac-insights-card"><div className="jac-insights-label">Dictations</div><strong>{stats[1]}</strong><p>Completed sessions saved on this device.</p></article>
            <article className="jac-insights-card"><div className="jac-insights-label">Total words dictated</div><strong>{stats[0]}</strong><p>Your words stay in your local history.</p></article>
        </div>
        <div className="jac-insights-lower">
            <article className="jac-insights-card jac-activity-card"><div className="jac-insights-label">7-day activity <span>{stats[4]} words today</span></div><div className="jac-activity-chart">{activity_bars}</div></article>
            <article className="jac-insights-card jac-streak-card"><div className="jac-insights-label">Streak</div><strong>🔥 {stats[5]} {streak_label}</strong><p>Saved you {str(int(int(stats[7]) / 60))} minutes vs typing · your activity stays local.</p><div className="jac-streak-dots">{streak_dots}</div></article>
        </div>
    </section>;
}

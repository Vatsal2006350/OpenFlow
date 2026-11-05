"""Pure local dictation metrics shared by Jac client screens."""

cl def:pub summarize_stats(records: list[dict], now_unix: int, tz_offset_minutes: int) -> list[str] {
    total_words = 0;
    total_secs = 0.0;
    total_sessions = 0;
    best_wpm = 0;
    words_today = 0;
    secs_today = 0.0;
    last7_words: list[int] = [0, 0, 0, 0, 0, 0, 0];
    today = int((now_unix - tz_offset_minutes * 60) / 86400);
    active_days: list[int] = [];
    for row in records {
        if len(row) == 0 { continue; }
        total_sessions += 1;
        timestamp = int(row.get("ts_unix", 0));
        words = int(row.get("words", 0));
        duration_ms = int(row.get("duration_ms", 0));
        seconds = duration_ms / 1000.0;
        total_words += words;
        total_secs += seconds;
        day = int((timestamp - tz_offset_minutes * 60) / 86400);
        if day not in active_days { active_days.append(day); }
        ago = today - day;
        if ago >= 0 and ago < 7 { last7_words[6 - ago] += words; }
        if words >= 3 and duration_ms >= 1000 and seconds > 0 {
            best_wpm = max(best_wpm, int(words * 60.0 / seconds + 0.5));
        }
        if day == today {
            words_today += words;
            secs_today += seconds;
        }
    }
    average_wpm = int(total_words * 60.0 / total_secs + 0.5) if total_secs > 0 else 0;
    today_wpm = int(words_today * 60.0 / secs_today + 0.5) if secs_today > 0 else 0;
    time_saved_secs = max(0.0, total_words / 45.0 * 60.0 - total_secs);
    day_streak = 0;
    day_cursor = today if today in active_days else today - 1;
    while day_cursor in active_days {
        day_streak += 1;
        day_cursor -= 1;
    }
    result: list[str] = [
        str(total_words), str(total_sessions), str(average_wpm), str(best_wpm),
        str(words_today), str(day_streak), str(today_wpm), str(int(time_saved_secs + 0.5))
    ];
    for value in last7_words { result.append(str(value)); }
    return result;
}

cl def:pub dictation_record(text: str, duration_secs: float, ts_unix: int, app: object) -> dict | None {
    words = len(text.split());
    if words == 0 { return None; }
    return {
        "ts_unix": ts_unix,
        "words": words,
        "duration_ms": int(round(max(0.0, duration_secs) * 1000.0)),
        "chars": len(text),
        "text": text,
        "app": app
    };
}

cl def:pub recent_history(records: list[dict], limit: int = 200) -> list[dict] {
    history: list[dict] = [];
    index = len(records) - 1;
    while index >= 0 and len(history) < limit {
        row = records[index];
        text = str(row.get("text", ""));
        if text {
            history.append({
                "ts_unix": int(row.get("ts_unix", 0)),
                "text": text,
                "app": row.get("app", None),
                "words": int(row.get("words", 0))
            });
        }
        index -= 1;
    }
    return history;
}

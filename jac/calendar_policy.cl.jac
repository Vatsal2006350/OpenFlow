"""Portable selection and reminder policy for locally fetched calendar data."""

cl def:pub calendar_start_ms(event: dict) -> int {
    value = str(event.get("start", ""));
    return int(Date.parse(value)) if value else 0;
}

cl def:pub calendar_query_window(now_ms: int) -> dict {
    return {
        "time_min": Date(now_ms).toISOString(),
        "time_max": Date(now_ms + 7 * 24 * 60 * 60 * 1000).toISOString()
    };
}

cl def:pub normalize_google_calendar_event(item: dict, account_email: str) -> dict {
    event_id = str(item.get("id", "")).strip();
    start_raw = item.get("start", {});
    end_raw = item.get("end", {});
    if start_raw is None or end_raw is None { return {}; }
    start_value: dict = dict(start_raw);
    end_value: dict = dict(end_raw);
    start = str(start_value.get("dateTime", start_value.get("date", ""))).strip();
    end = str(end_value.get("dateTime", end_value.get("date", ""))).strip();
    if not event_id or not start or not end { return {}; }
    raw_title = item.get("summary", None);
    title = str(raw_title).strip() if raw_title is not None else "";
    if not title { title = "Untitled meeting"; }
    raw_join_url = item.get("hangoutLink", None);
    join_url = str(raw_join_url).strip() if raw_join_url is not None else "";
    status = str(item.get("status", "")).strip().lower();
    return {
        "id": event_id,
        "title": title,
        "start": start,
        "end": end,
        "account_email": account_email,
        "join_url": join_url if join_url else None,
        "cancelled": status == "cancelled"
    };
}

cl def:pub select_upcoming_calendar_events(events: list[dict], now_ms: int) -> list[dict] {
    selected: list[dict] = [];
    for event in events {
        if bool(event.get("cancelled", False)) { continue; }
        start_ms = calendar_start_ms(event);
        if start_ms > 0 and start_ms >= now_ms { selected.append(dict(event)); }
    }
    selected.sort(key=lambda (event: dict) { return calendar_start_ms(event); });
    return selected;
}

cl def:pub calendar_reminder_due(event: dict, now_ms: int) -> bool {
    start_ms = calendar_start_ms(event);
    return not bool(event.get("cancelled", False)) and start_ms >= now_ms and start_ms <= now_ms + 60_000;
}

cl def:pub calendar_reminder_key(event: dict) -> str {
    return str(event.get("account_email", "")) + ":" + str(event.get("id", ""));
}

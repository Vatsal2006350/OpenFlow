"""Read and search the local OpenFlow meeting archive."""

import json;
import os;
import sys;
import from typing { Any }


def app_support_dir() -> str {
    if sys.platform == "darwin" {
        return os.path.join(os.path.expanduser("~"), "Library", "Application Support", "OpenFlow");
    }
    if sys.platform == "win32" {
        return os.path.join(os.environ.get("APPDATA", os.path.expanduser("~")), "OpenFlow");
    }
    data_home = os.environ.get("XDG_DATA_HOME", os.path.join(os.path.expanduser("~"), ".local", "share"));
    return os.path.join(data_home, "OpenFlow");
}


def store_path() -> str {
    return os.path.join(app_support_dir(), "meetings.json");
}


def list_meetings() -> list[dict[str, Any]] {
    path = store_path();
    if not os.path.isfile(path) {
        return [];
    }
    with open(path, "r", encoding="utf-8") as handle {
        data: Any = json.load(handle);
    }
    meetings: list[dict[str, Any]] = [];
    if isinstance(data, list) {
        for item in data {
            if isinstance(item, dict) {
                meeting: dict[str, Any] = item;
                meetings.append(meeting);
            }
        }
    }
    meetings.sort(key=meeting_created_at, reverse=True);
    return meetings;
}


def search_meetings(query: str, limit: int = 10) -> list[dict[str, Any]] {
    terms = query.casefold().split();
    if not terms or limit <= 0 {
        return [];
    }
    hits: list[dict[str, Any]] = [];
    for meeting in list_meetings() {
        haystack = " ".join([
            meeting.get("title", ""),
            meeting.get("summary", ""),
            meeting.get("transcript", "")
        ]).casefold();
        score = sum(1 for term in terms if term in haystack);
        if score > 0 {
            hits.append({"score": score, "meeting": meeting});
        }
    }
    hits.sort(key=meeting_search_rank);
    results: list[dict[str, Any]] = [];
    for item in hits[:limit] {
        results.append(item["meeting"]);
    }
    return results;
}


def meeting_created_at(meeting: dict[str, Any]) -> int {
    return int(meeting.get("created_at", 0));
}


def meeting_search_rank(hit: dict[str, Any]) -> tuple[int, int] {
    meeting: dict[str, Any] = hit["meeting"];
    return (-int(hit["score"]), -meeting_created_at(meeting));
}


def read_meeting(meeting_id: str) -> dict[str, Any] | None {
    for meeting in list_meetings() {
        if meeting.get("id", "") == meeting_id {
            return meeting;
        }
    }
    return None;
}

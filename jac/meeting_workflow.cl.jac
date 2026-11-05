"""Jac-owned meeting input, local summarization, record shaping, and save flow."""

cl import from "@tauri-apps/api/core" { invoke }
cl import from .cleanup_policy { cleanup_passes }
cl import from .meeting_policy { prepare_meeting_input, meeting_summary_messages, meeting_summary_fallback }

cl async def:pub process_and_save_meeting(title: str, transcript: str, thoughts: str = "") -> dict {
    prepared = prepare_meeting_input(title, transcript);
    if not bool(prepared.get("accepted", False)) {
        return {"error": str(prepared.get("error", "No speech was recognized."))};
    }

    source = str(prepared["transcript"]);
    result = meeting_summary_fallback(source, len(source.split()) < 25);
    if len(source.split()) >= 25 {
        try {
            response: str = str(await invoke("run_cleanup_with_messages", {
                "provider": "local",
                "raw": source,
                "messages": meeting_summary_messages(source)
            }));
            candidate: str = response.strip();
            if candidate[0:7] == "```json" { candidate = candidate[7:]; }
            elif candidate[0:3] == "```" { candidate = candidate[3:]; }
            if len(candidate) >= 3 and candidate[len(candidate) - 3:] == "```" { candidate = candidate[:len(candidate) - 3]; }
            decoded: dict = dict(JSON.parse(candidate.strip()));
            formatted = str(decoded.get("transcript", "")).strip();
            summary = str(decoded.get("summary", "")).strip();
            if formatted and summary and cleanup_passes(source, formatted, "light") {
                result = {"transcript": formatted, "summary": summary, "status": "generated"};
            }
        } except Exception {
            # Keep the recognized transcript when local formatting is unavailable.
        }
    }

    now_ms = int(Date.now());
    record = {
        "id": str(now_ms) + "-" + str(Math.random()),
        "title": str(prepared["title"]),
        "transcript": str(result["transcript"]),
        "summary": str(result["summary"]),
        "summary_status": str(result["status"]),
        "thoughts": thoughts.strip(),
        "created_at": int(now_ms / 1000)
    };
    try {
        saved: dict = dict(await invoke("persist_meeting_record", {"record": record}));
        return {"meeting": saved};
    } except Exception as err {
        return {"error": str(err), "meeting": record};
    }
}

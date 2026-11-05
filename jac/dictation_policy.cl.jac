"""Pure dictation state transitions. Hardware and pipeline effects stay native."""

cl def:pub reduce_dictation(machine: dict, event: dict) -> list[dict] {
    actions: list[dict] = [];
    kind = str(event.get("kind", ""));
    phase = str(machine.get("phase", "idle"));
    now = int(event.get("at_ms", 0));
    mode = str(machine.get("mode", ""));
    session = int(machine.get("session", 0));
    started = int(machine.get("started_ms", 0));
    last_end = int(machine.get("last_end_ms", -1000000));
    tap_up = int(machine.get("tap_up_ms", 0));
    warned = bool(machine.get("warned", False));
    binding = str(event.get("binding", ""));

    def begin(next_mode: str, at_ms: int) -> list[dict] {
        next_session = int(machine.get("next_session", 1));
        machine["next_session"] = next_session + 1;
        machine["phase"] = "recording";
        machine["mode"] = next_mode;
        machine["session"] = next_session;
        machine["started_ms"] = at_ms;
        machine["warned"] = False;
        bar = "recording" if next_mode == "push_to_talk" else "locked";
        actions = [{"kind": "start_capture", "session": next_session, "mode": next_mode}];
        if bool(machine.get("sound_on_start", True)) { actions.append({"kind": "play_ping"}); }
        actions.append({"kind": "show_bar", "state": bar});
        return actions;
    }

    def finalize(active_session: int) -> list[dict] {
        machine["phase"] = "finalizing";
        machine["session"] = active_session;
        return [
            {"kind": "stop_capture_and_finalize", "session": active_session},
            {"kind": "show_bar", "state": "transcribing"},
            {"kind": "run_pipeline", "session": active_session}
        ];
    }

    if kind == "trigger_down" {
        if phase == "idle" {
            if now - last_end < 500 { return actions; }
            if binding == "push_to_talk" { return begin("push_to_talk", now); }
            if binding == "hands_free" { return begin("locked", now); }
        }
        if phase == "recording" and mode == "push_to_talk" and binding == "hands_free" {
            machine["mode"] = "locked";
            return [{"kind": "show_bar", "state": "locked"}];
        }
        if phase == "awaiting_lock" and binding == "push_to_talk" and now - tap_up <= 350 {
            return begin("locked", now);
        }
        if phase == "awaiting_lock" and binding == "hands_free" {
            return begin("locked", now);
        }
        if phase == "recording" and mode == "locked" and (binding == "push_to_talk" or binding == "hands_free") {
            return finalize(session);
        }
    }

    if kind == "trigger_up" and phase == "recording" and mode == "push_to_talk" and binding == "push_to_talk" {
        if now - started >= 200 { return finalize(session); }
        machine["phase"] = "awaiting_lock";
        machine["tap_up_ms"] = now;
        return [
            {"kind": "discard_capture", "session": session},
            {"kind": "show_bar", "state": "idle"}
        ];
    }

    if kind == "cancel" {
        if phase == "recording" or phase == "finalizing" {
            actions.append({"kind": "discard_capture", "session": session});
        }
        if phase != "idle" {
            machine["phase"] = "idle";
            machine["last_end_ms"] = now;
            actions.append({"kind": "show_bar", "state": "cancelled"});
            actions.append({"kind": "show_bar", "state": "idle"});
        }
        return actions;
    }

    if kind == "stop" and phase == "recording" { return finalize(session); }
    if kind == "pipeline_committed" and phase == "finalizing" and int(event.get("session", 0)) == session {
        machine["phase"] = "idle";
        machine["last_end_ms"] = now;
        return [{"kind": "show_bar", "state": "done"}];
    }
    if kind == "pipeline_failed" and phase == "finalizing" and int(event.get("session", 0)) == session {
        machine["phase"] = "idle";
        machine["last_end_ms"] = now;
        return [{"kind": "show_bar", "state": "idle"}];
    }
    if kind == "tick" and phase == "awaiting_lock" and now - tap_up > 350 {
        machine["phase"] = "idle";
        machine["last_end_ms"] = now;
        return [{"kind": "show_bar", "state": "idle"}];
    }
    if kind == "tick" and phase == "recording" {
        elapsed = now - started;
        if elapsed >= 1200000 { return finalize(session); }
        if elapsed >= 1140000 and not warned {
            machine["warned"] = True;
            return [{"kind": "warn_session_cap"}];
        }
    }
    return actions;
}

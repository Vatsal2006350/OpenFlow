"""Pure Publik balance and usage presentation rules."""

cl def:pub publik_why_costs() -> str {
    return "A provider charges for every request the app makes; publik pays that bill and passes it on at half the provider's list price. Nothing is charged behind your back — usage only draws from a plan or pack you choose to buy.";
}

cl def:pub publik_first_run_view(status: dict) -> dict {
    if not bool(status.get("first_run_pending", False)) or not bool(status.get("has_key", False)) { return {}; }
    balance = status.get("starter_micros", 0);
    if "balance_micros" in status { balance = status.get("balance_micros", 0); }
    claimed = str(status.get("claim_state", "")) == "claimed";
    balance_value = int(balance or 0);
    balance_line = "$0.00 · link this computer for $0.05 of free use" if balance_value <= 0 and not claimed else f"{publik_money(balance_value)} on your publik balance";
    return {
        "balance_line": balance_line,
        "justification": publik_why_costs(),
        "cta_label": "Link this computer & pick a plan",
        "later_label": "Later"
    };
}

cl def:pub publik_plan_button(status: dict) -> dict {
    claimed = str(status.get("claim_state", "")) == "claimed";
    return {"label": "Manage plan" if claimed else "Pick a plan", "claimed": claimed};
}

cl def:pub publik_action_url(kind: str, status: dict) -> str {
    dashboard = str(status.get("dashboard_url", "https://publikhq.com/dashboard/api"));
    if not dashboard { dashboard = "https://publikhq.com/dashboard/api"; }
    claim = str(status.get("claim_url", ""));
    if claim == "None" { claim = ""; }
    top_up = str(status.get("top_up_url", ""));
    if top_up == "None" { top_up = ""; }
    if kind == "first_run" or kind == "claim" { return claim if claim else dashboard; }
    if kind == "plan" { return dashboard if str(status.get("claim_state", "")) == "claimed" or not claim else claim; }
    if kind == "top_up" { return top_up if top_up else claim if claim else dashboard; }
    if kind == "terms" { return "https://publikhq.com/terms"; }
    return dashboard;
}

cl def:pub publik_money(micros: int) -> str {
    amount = float(micros) / 1000000.0;
    if abs(amount) < 0.01 {
        return f"${amount:.4f}";
    }
    return f"${amount:.2f}";
}

cl def:pub publik_balance_label(micros: object) -> str {
    if micros is None { return ""; }
    value = int(micros);
    if value <= 0 { return "$0.00 left"; }
    return f"{publik_money(value)} left";
}

cl def:pub publik_week_label(used: object, budget: object) -> str {
    if used is None { return ""; }
    used_value = int(used);
    if budget is not None and int(budget) > 0 {
        return f"This week {publik_money(used_value)} of {publik_money(int(budget))}";
    }
    return f"{publik_money(used_value)} used this week";
}

cl def:pub publik_last_charge_label(charge: object) -> str {
    if charge is None { return ""; }
    return f"last cleanup {publik_money(int(charge))}";
}

cl def:pub publik_provision_error(raw_error: str) -> str {
    prefix = "WF_PUBLIK_PROVISION_ERROR:";
    pieces: list[str] = raw_error.split(prefix);
    if len(pieces) < 2 { return raw_error; }
    try {
        fact: dict = dict(JSON.parse(pieces[len(pieces) - 1]));
        kind = str(fact.get("kind", ""));
        if kind == "rate_limited" {
            retry_value = fact.get("retry_after_secs", None);
            if retry_value is None { return "publik is busy right now — try again in a few minutes."; }
            minutes = max(1, int(retry_value) // 60);
            return f"publik is busy right now — try again in {minutes} minutes.";
        }
        if kind == "unavailable" { return "publik API is unreachable right now. Nothing is being charged. Try again in a minute, or use your own key."; }
        if kind == "transport" { return "Could not reach publikhq.com. Check your connection and try again, or use your own key."; }
        if kind == "http" {
            detail = str(fact.get("detail", ""));
            try {
                body: dict = dict(JSON.parse(detail));
                error_body: dict = dict(body.get("error", {}));
                message = str(error_body.get("message", "")).strip();
                if message { return message; }
            } except Exception { }
            return f"publik answered HTTP {int(fact.get('status', 0))}.";
        }
        provider_message = str(fact.get("message", "")).strip();
        if provider_message { return provider_message; }
        if kind == "invalid_key" { return "publik API rejected the key. Try again or use your own key."; }
        return "publik could not set up this device. Try again, or use your own key.";
    } except Exception {
        return "publik could not set up this device. Try again, or use your own key.";
    }
}

cl def:pub publik_low_starter_notice(status: dict) -> dict {
    remaining = status.get("starter_remaining_micros", None);
    if remaining is None { return {}; }
    remaining_value = int(remaining);
    grant_value = status.get("starter_granted_micros", 0);
    granted = int(grant_value) if grant_value is not None else 0;
    if granted <= 0 { granted = 50_000; }
    if remaining_value <= 0 or remaining_value * 5 >= granted { return {}; }
    return {
        "kind": "low_starter",
        "headline": "publik API free use is running low",
        "message": f"{publik_money(remaining_value)} of your free use is left. {publik_why_costs()} Pick a plan or a pack to keep cloud cleanup going, or use Local or your own key.",
        "link_label": "Pick a plan",
        "link_url": publik_action_url("top_up", status)
    };
}

cl def:pub publik_exhausted_notice(status: dict) -> dict {
    message = str(status.get("exhausted_message", "")).strip();
    if not message { return {}; }
    claimed = str(status.get("claim_state", "")) == "claimed";
    return {
        "kind": "exhausted",
        "headline": "publik API needs a plan or a pack" if claimed else "publik API needs a balance",
        "message": f"{message} Dictation still works; text is pasted without cleanup — or use Local or your own key under Settings → Cleanup Engine.",
        "link_label": "Add a plan or pack" if claimed else "Link this computer & pick a plan",
        "link_url": publik_action_url("top_up", status)
    };
}

"""Jac client dictionary editor backed by the local Tauri storage adapter."""

cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }

glob ASCII_PUNCTUATION: str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

cl def:pub merge_dictionary_entry(entries: list[dict], correct: str, mishears: list[str], auto: bool) -> list[dict] {
    merged: list[dict] = [];
    found = False;
    target = correct.lower();
    for entry in entries {
        existing_correct = str(entry.get("correct", ""));
        if existing_correct.lower() == target {
            merged_mishears: list[str] = [];
            for value in list(entry.get("mishears", [])) { merged_mishears.append(str(value)); }
            for mishear in mishears {
                already_present = False;
                for existing in merged_mishears {
                    if existing.lower() == mishear.lower() { already_present = True; break; }
                }
                if not already_present { merged_mishears.append(mishear); }
            }
            merged.append({"correct": existing_correct, "mishears": merged_mishears, "auto": bool(entry.get("auto", auto))});
            found = True;
        } else { merged.append(dict(entry)); }
    }
    if not found { merged.append({"correct": correct, "mishears": mishears, "auto": auto}); }
    return merged;
}

cl def:pub remove_dictionary_entry(entries: list[dict], correct: str) -> list[dict] {
    kept: list[dict] = [];
    target = correct.lower();
    for entry in entries {
        if str(entry.get("correct", "")).lower() != target { kept.append(dict(entry)); }
    }
    return kept;
}

cl def:pub dictionary_edit_distance(left: str, right: str) -> int {
    previous: list[int] = [];
    for index in range(len(right) + 1) { previous.append(index); }
    for row in range(1, len(left) + 1) {
        current: list[int] = [row];
        for column in range(1, len(right) + 1) {
            substitution = 0 if left[row - 1] == right[column - 1] else 1;
            current.append(min(
                current[column - 1] + 1,
                previous[column] + 1,
                previous[column - 1] + substitution
            ));
        }
        previous = current;
    }
    return previous[len(right)];
}

cl def:pub dictionary_match_close(left: str, right: str) -> bool {
    if left == right { return True; }
    max_length = max(len(left), len(right));
    if max_length == 0 { return False; }
    length_difference = len(left) - len(right);
    if length_difference < 0 { length_difference = -length_difference; }
    if length_difference / max_length > 0.34 { return False; }
    return dictionary_edit_distance(left, right) / max_length <= 0.34;
}

cl def:pub relevant_vocabulary(utterance: str, entries: list[dict], limit: int = 15) -> list[dict] {
    tokens: list[str] = [];
    for token in utterance.split() {
        normalized = str(token).strip(ASCII_PUNCTUATION).lower();
        if normalized { tokens.append(normalized); }
    }
    grams: list[str] = list(tokens);
    for index in range(len(tokens) - 1) {
        grams.append(tokens[index] + tokens[index + 1]);
    }

    selected: list[dict] = [];
    for entry in entries {
        correct = str(entry.get("correct", ""));
        targets: list[str] = [correct.lower()];
        mishears: list[str] = [];
        for raw_mishear in list(entry.get("mishears", [])) {
            mishear = str(raw_mishear);
            mishears.append(mishear);
            targets.append(mishear.lower());
        }
        matched = False;
        for gram in grams {
            for target in targets {
                if dictionary_match_close(gram, target) { matched = True; break; }
            }
            if matched { break; }
        }
        if matched {
            selected.append({"correct": correct, "mishears": mishears});
            if len(selected) >= limit { break; }
        }
    }
    return selected;
}

cl def:pub Dictionary() -> JsxElement {
    has entries: list[dict] = [];
    has query: str = "";
    has tab: str = "all";
    has adding: bool = False;
    has correct: str = "";
    has heard: str = "";
    has error: str = "";
    has busy: bool = False;

    async def reload() {
        try { entries = list(await invoke("get_dictionary")); }
        except Exception { entries = []; }
    }

    useEffect(lambda { reload(); }, []);

    async def save_entry() {
        word = correct.strip();
        if not word or busy { return; }
        mishears: list[str] = [];
        for token in heard.split(",") {
            value = token.strip();
            if value { mishears.append(value); }
        }
        error = "";
        busy = True;
        try {
            current_entries: list[dict] = list(await invoke("get_dictionary"));
            next_entries = merge_dictionary_entry(current_entries, word, mishears, False);
            await invoke("save_dictionary_entries", {"entries": next_entries});
            correct = "";
            heard = "";
            adding = False;
            entries = next_entries;
        } except Exception as err { error = str(err); }
        busy = False;
    }

    async def remove_entry(word: str) {
        try {
            current_entries: list[dict] = list(await invoke("get_dictionary"));
            next_entries = remove_dictionary_entry(current_entries, word);
            await invoke("save_dictionary_entries", {"entries": next_entries});
            entries = next_entries;
        } except Exception as err { error = str(err); }
    }

    filtered: list[dict] = [];
    q = query.strip().lower();
    for item in entries {
        correct_word = str(item.get("correct", ""));
        matches = not q or correct_word.lower().find(q) >= 0;
        is_auto = bool(item.get("auto", False));
        if matches and (tab == "all" or (tab == "personal" and not is_auto)) { filtered.append(item); }
    }
    entry_rows: list[JsxElement] = [];
    for item in filtered {
        word = str(item.get("correct", ""));
        mishear_names: list[str] = [];
        mishears: list[str] = list(item.get("mishears", []));
        for mishear in mishears { mishear_names.append(str(mishear)); }
        entry_rows.append(<div className="jac-dictionary-row" key={word}>
            <div><strong>{word}</strong>{bool(item.get("auto", False)) and <span className="jac-auto-badge">Auto-learned</span>}{len(mishear_names) > 0 and <small>Also heard as {", ".join(mishear_names)}</small>}</div>
            <button aria-label={"Remove " + word} title="Remove" onClick={lambda { remove_entry(word); }}>×</button>
        </div>);
    }

    return <section className="jac-dictionary">
        <header className="jac-dictionary-heading"><div><div className="jac-home-eyebrow">Personal vocabulary</div><h1>Dictionary</h1><p>Teach OpenFlow the names and jargon it should get right.</p></div><button className="jac-primary-button" onClick={lambda { adding = not adding; }}>＋ Add new</button></header>
        <nav className="jac-dictionary-tabs" aria-label="Dictionary category">
            <button className={"active" if tab == "all" else ""} onClick={lambda { tab = "all"; }}>All</button>
            <button className={"active" if tab == "personal" else ""} onClick={lambda { tab = "personal"; }}>Personal</button>
            <button className={"active" if tab == "shared" else ""} onClick={lambda { tab = "shared"; }}>Shared with team</button>
        </nav>
        {adding and <section className="jac-dictionary-form">
            <strong>Add a word</strong>
            <label>Word<input autoFocus value={correct} onChange={lambda (event: dict) { correct = str(event["target"]["value"]); }} placeholder="e.g. OpenFlow" /></label>
            <label>Also heard as <span>(optional, comma-separated)</span><input value={heard} onChange={lambda (event: dict) { heard = str(event["target"]["value"]); }} placeholder="whisper flow, wimper flow" /></label>
            {error and <p className="jac-dictionary-error">{error}</p>}
            <div><button className="jac-primary-button" disabled={busy or not correct.strip()} onClick={lambda { save_entry(); }}>{"Adding…" if busy else "Add word"}</button><button className="jac-secondary-button" onClick={lambda { adding = False; }}>Cancel</button></div>
        </section>}
        <div className="jac-dictionary-tools"><input aria-label="Search dictionary" value={query} onChange={lambda (event: dict) { query = str(event["target"]["value"]); }} placeholder="Search words" /></div>
        {error and not adding and <p className="jac-dictionary-error">{error}</p>}
        <section className="jac-dictionary-list">
            {tab == "shared" and <div className="jac-dictionary-empty">Team sharing is coming soon.</div>}
            {tab != "shared" and len(filtered) == 0 and <div className="jac-dictionary-empty">{"No words yet. Add one, or OpenFlow will learn the terms you correct." if len(entries) == 0 else "No words match your search."}</div>}
            {tab != "shared" and entry_rows}
        </section>
    </section>;
}

"""Evaluate native Accessibility snapshots and persist eligible corrections."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .autolearn_policy { detect_correction }
cl import from .dictionary { merge_dictionary_entry }

cl def:pub AutoLearnBridge() -> JsxElement {
    async def process(event: dict) {
        payload: dict = dict(event.get("payload", {}));
        inserted = str(payload.get("inserted", ""));
        after = str(payload.get("after", ""));
        correction = detect_correction(inserted, after);
        if correction {
            try {
                entries: list[dict] = list(await invoke("get_dictionary"));
                updated = merge_dictionary_entry(
                    entries,
                    str(correction["correct"]),
                    [str(correction["mishear"])],
                    True
                );
                await invoke("save_dictionary_entries", {"entries": updated});
            } except Exception { }
        }
    }
    useEffect(lambda {
        async def subscribe() {
            try { await listen("openflow://autolearn/correction", lambda (event: dict) { process(event); }); }
            except Exception { }
        }
        subscribe();
    }, []);
    return <span aria-hidden="true" className="jac-autolearn-bridge" />;
}

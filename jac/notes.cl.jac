"""Jac client for the local scratchpad."""

cl import from react { useEffect }
cl import from "@tauri-apps/api/core" { invoke }

cl def:pub Notes() -> JsxElement {
    has text: str = "";
    has loaded: bool = False;
    has saved: bool = True;
    async def load_notes() {
        try { text = str(await invoke("get_scratchpad")); }
        except Exception { text = ""; }
        loaded = True;
    }
    useEffect(lambda {
        load_notes();
    }, []);
    useEffect(lambda {
        if not loaded { return; }
        saved = False;
        timer = setTimeout(lambda {
            async def persist_notes() {
                try { await invoke("save_scratchpad", {"text": text}); saved = True; }
                except Exception { saved = False; }
            }
            persist_notes();
        }, 250);
        return lambda { clearTimeout(timer); };
    }, [text, loaded]);

    return <section className="jac-notes">
        <header><div className="jac-home-eyebrow">Your space</div><h1>Notes</h1><p>A private scratchpad for thoughts and drafts. It saves automatically on this device.</p></header>
        <textarea aria-label="Scratchpad notes" value={text} onChange={lambda (event: dict) { text = str(event["target"]["value"]); }} placeholder="Start writing…" />
        <div className="jac-notes-save">{ "Saved locally" if saved else "Saving…" }</div>
    </section>;
}

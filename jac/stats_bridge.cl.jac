"""Shape completed dictation records in Jac before native local persistence."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .stats { dictation_record }

cl def:pub StatsBridge() -> JsxElement {
    async def process_record(event: dict) {
        payload: dict = dict(event.get("payload", {}));
        request_id = str(payload.get("request_id", ""));
        shaped = dictation_record(
            str(payload.get("text", "")),
            float(payload.get("duration_secs", 0.0)),
            int(payload.get("ts_unix", 0)),
            payload.get("app", None)
        );
        try {
            await invoke("resolve_jac_stats_record", {"requestId": request_id, "record": shaped});
        } except Exception { }
    }
    useEffect(lambda {
        async def subscribe() {
            try {
                await listen("openflow://stats/record", lambda (event: dict) { process_record(event); });
                await invoke("register_jac_stats");
            } except Exception { }
        }
        subscribe();
        return lambda { invoke("unregister_jac_stats"); };
    }, []);
    return <span aria-hidden="true" className="jac-stats-bridge" />;
}

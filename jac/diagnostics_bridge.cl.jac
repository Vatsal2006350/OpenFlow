"""Resolve diagnostic messages in Jac and delegate desktop effects to Tauri."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect, useRef }
cl import from .diagnostics_policy { diagnostic_for }

cl def:pub DiagnosticsBridge() -> JsxElement {
    generation_ref = useRef(0);
    useEffect(lambda {
        async def subscribe() {
            try {
                await listen("openflow://diagnostic/request", lambda (event: dict) {
                    payload: dict = event["payload"];
                    code = str(payload.get("code", ""));
                    platform = str(payload.get("platform", "macos"));
                    copy = diagnostic_for(code, platform);
                    headline = str(payload.get("headline", copy["headline"]));
                    detail = str(payload.get("detail", copy["detail"]));
                    generation_ref.current = generation_ref.current + 1;
                    current = generation_ref.current;
                    async def present() {
                        try { await invoke("report_jac_diagnostic", {"code": code, "headline": headline, "detail": detail}); }
                        except Exception { }
                        async def expire() {
                            if generation_ref.current == current {
                                try { await invoke("set_flowbar_idle"); } except Exception { }
                            }
                        }
                        setTimeout(expire, 4500);
                    }
                    present();
                });
            } except Exception { }
        }
        subscribe();
        return lambda { };
    }, []);
    return <span aria-hidden="true" className="jac-diagnostics-bridge" />;
}

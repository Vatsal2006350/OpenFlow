"""Connect local native audio measurements to Jac's quality policy."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from "react" { useEffect }
cl import from .audio_policy { assess_dictation_audio }

cl def:pub AudioPolicyBridge() -> JsxElement {
    async def assess(event: dict) {
        payload: dict = dict(event.get("payload", {}));
        result = assess_dictation_audio(
            float(payload.get("peak", 0.0)),
            float(payload.get("rms", 0.0)),
            float(payload.get("duration_secs", 0.0))
        );
        try {
            await invoke("resolve_jac_audio_policy", {
                "request_id": str(payload.get("request_id", "")),
                "accepted": bool(result.get("accepted", False)),
                "real_attempt": bool(result.get("real_attempt", False))
            });
        } except Exception { }
    }
    useEffect(lambda {
        async def subscribe() {
            try {
                await listen("openflow://audio/policy_request", lambda (event: dict) { assess(event); });
                await invoke("register_jac_audio_policy");
            } except Exception { }
        }
        subscribe();
        return lambda { invoke("unregister_jac_audio_policy"); };
    }, []);
    return <span aria-hidden="true" className="jac-audio-policy-bridge" />;
}

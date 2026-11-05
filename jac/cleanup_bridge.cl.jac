"""Routes native ASR results through Jac cleanup policy and native providers."""

cl import from "@tauri-apps/api/event" { listen }
cl import from "@tauri-apps/api/core" { invoke }
cl import from react { useEffect }
cl import from .cleanup_policy { build_cleanup_messages, build_cleanup_provider_payload, cleanup_bypasses_model, cleanup_passes, cleanup_pre_normalize_layout, cleanup_post_process, cleanup_max_tokens, decode_cleanup_provider_response, decode_cloud_asr_response }
cl import from .dictionary { relevant_vocabulary }

cl def:pub CleanupBridge() -> JsxElement {
    async def process_cleanup(event: dict) {
        payload: dict = event["payload"];
        request_id = str(payload.get("request_id", ""));
        source = str(payload.get("transcript", ""));
        raw_fallback = str(payload.get("raw_fallback", source));
        cloud_asr_failed = False;
        if bool(payload.get("cloud_asr_response", False)) {
            decoded_asr = decode_cloud_asr_response(source);
            if bool(decoded_asr.get("accepted", False)) {
                source = str(decoded_asr.get("text", ""));
                raw_fallback = source;
            } else {
                cloud_asr_failed = True;
            }
        }
        provider = str(payload.get("cleanup_mode", "local"));
        level = str(payload.get("cleanup_level", "light"));
        final_text = "" if cloud_asr_failed else raw_fallback;
        if not cloud_asr_failed and provider != "raw" and not cleanup_bypasses_model(level) and source.strip() {
            raw = cleanup_pre_normalize_layout(source);
            raw_fallback = cleanup_post_process(raw);
            try {
                messages = build_cleanup_messages(
                    raw,
                    level,
                    relevant_vocabulary(source, list(payload.get("dictionary", []))),
                    str(payload.get("app_bundle_id", "")),
                    str(payload.get("window_context", ""))
                );
                max_tokens = cleanup_max_tokens(raw, 512);
                model = str(payload.get("openai_model", ""));
                if provider == "anthropic" {
                    model = str(payload.get("anthropic_model", ""));
                }
                request_body = build_cleanup_provider_payload(
                    provider,
                    model,
                    max_tokens,
                    messages
                );
                candidate = "";
                try {
                    response = str(await invoke("run_cleanup_with_messages", {
                        "provider": provider,
                        "raw": raw,
                        "maxTokens": max_tokens,
                        "messages": messages,
                        "requestBody": request_body
                    }));
                    if provider == "open_ai" or provider == "anthropic" {
                        decoded = decode_cleanup_provider_response(provider, response);
                        if bool(decoded.get("accepted", False)) {
                            candidate = str(decoded.get("text", ""));
                        }
                    } else {
                        candidate = response;
                    }
                } except Exception { }
                if not candidate.strip() and provider != "local" {
                    try {
                        candidate = str(await invoke("run_cleanup_with_messages", {
                            "provider": "local",
                            "raw": raw,
                            "maxTokens": max_tokens,
                            "messages": messages,
                            "requestBody": None
                        }));
                    } except Exception { }
                }
                if candidate.strip() {
                    candidate = cleanup_post_process(candidate);
                    if cleanup_passes(raw_fallback, candidate, level) { final_text = candidate; }
                }
            } except Exception {
                # Cleanup is an enhancement: native capture falls back to raw text.
            }
        }
        try { await invoke("resolve_jac_cleanup", {"requestId": request_id, "text": final_text}); }
        except Exception {}
    }

    useEffect(lambda {
        async def subscribe() {
            try {
                await listen("openflow://cleanup/request", lambda (event: dict) { process_cleanup(event); });
                await invoke("register_jac_cleanup");
            } except Exception {}
        }
        subscribe();
        return lambda {
            invoke("unregister_jac_cleanup");
        };
    }, []);
    return <span aria-hidden="true" className="jac-cleanup-bridge" />;
}

"""Portable model-download prompt and progress policy."""

cl def:pub model_policy_fields(platform: str) -> list[str] {
    fields: list[str] = [
        "ggml-base.en.bin",
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
        "147964211"
    ];
    if platform == "macos" {
        fields.extend(["ggml-large-v3-turbo.bin", "ggml-medium.en.bin", "ggml-small.en.bin", "ggml-base.en.bin"]);
    } else {
        fields.extend(["ggml-medium.en.bin", "ggml-small.en.bin", "ggml-base.en.bin"]);
    }
    return fields;
}

cl def:pub model_download_allowed(state: str) -> bool {
    return state == "missing" or state == "failed";
}

cl def:pub model_unavailable_action(state: str) -> str {
    if state == "missing" or state == "failed" { return "show_prompt"; }
    if state == "downloading" { return "explain_download"; }
    return "ignore";
}

cl def:pub reduce_model_state(state: list[str], input: list[str]) -> list[str] {
    if len(input) == 0 { return state; }
    kind = input[0];
    if kind == "ready" or kind == "missing" { return input; }
    if kind == "download_started" {
        if len(input) >= 2 { return ["downloading", "0", input[1]]; }
        return ["downloading", "0", "0"];
    }
    if kind == "download_progress" and len(input) >= 3 {
        return ["downloading", input[1], input[2]];
    }
    if kind == "download_failed" and len(input) >= 2 {
        return ["failed", input[1]];
    }
    if kind == "download_load_failed" { return ["failed", "load_failed"]; }
    return state;
}

cl def:pub model_progress(received: int, total: int) -> int {
    if total <= 0 { return 0; }
    value = int((received * 100) / total);
    return value if value < 100 else 100;
}

cl def:pub model_failure_message(failure: str) -> str {
    if failure == "network_unreachable" {
        return "OpenFlow could not reach huggingface.co. Check your internet connection, then try again.";
    }
    if failure.startswith("server_status:") {
        return "The download server answered " + failure[14:] + ". Try again later.";
    }
    if failure.startswith("download_client_failed:") {
        return "The secure download could not start. Try again.";
    }
    if failure == "download_interrupted" {
        return "The download stopped part way. Check your internet connection, then try again.";
    }
    if failure.startswith("save_failed:") {
        return "Could not save the model in OpenFlow's local model folder: " + failure[12:];
    }
    if failure == "checksum_mismatch" {
        return "The downloaded file did not pass its integrity check. Try again.";
    }
    if failure == "load_failed" {
        return "The model downloaded, but Whisper could not load it. Try again.";
    }
    return "The speech model could not be installed. Try again.";
}

cl def:pub model_megabytes(bytes: int) -> str {
    return str(int((bytes + 500000) / 1000000)) + " MB";
}

cl def:pub model_download_size_label() -> str {
    fields = model_policy_fields("macos");
    return model_megabytes(int(fields[3]));
}

cl def:pub model_download_file_label() -> str {
    return model_policy_fields("macos")[0];
}

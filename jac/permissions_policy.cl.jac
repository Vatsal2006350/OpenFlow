"""Microphone permission guidance copy from native permission facts."""

cl def:pub microphone_permission_hint(grant: str, charged_to: object) -> str {
    if grant == "granted" { return ""; }
    if charged_to is not None {
        other = str(charged_to);
        return f"macOS is judging this as {other}, not OpenFlow — switch {other} on in the Microphone list, or quit OpenFlow and reopen it from Applications so it answers for itself.";
    }
    if grant == "refused" {
        return "turned down earlier, so macOS won't ask again — switch OpenFlow on in the Microphone list.";
    }
    return "";
}

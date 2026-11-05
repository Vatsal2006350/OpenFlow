"""Local recording quality policy shared by desktop capture adapters."""

cl def:pub assess_dictation_audio(peak: float, rms: float, duration_secs: float) -> dict {
    real_attempt = duration_secs >= 0.6;
    accepted = peak >= 0.005 and (rms >= 0.002 or peak >= 0.02);
    reason = "accepted" if accepted else "below_speech_floor";
    return {"accepted": accepted, "real_attempt": real_attempt, "reason": reason};
}

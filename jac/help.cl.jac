"""Local usage guidance for OpenFlow."""

cl def:pub Help() -> JsxElement {
    tips: list[dict] = [
        {"emoji": "🎙️", "title": "Hold to dictate", "body": "Press and hold your configured dictation key, speak naturally, then release. OpenFlow transcribes on this device unless you choose a cloud transcription engine."},
        {"emoji": "✨", "title": "Text goes where your cursor is", "body": "Release the key and your text is inserted into the app that has focus. Choose how much cleanup to apply under Settings."},
        {"emoji": "📖", "title": "Teach it your vocabulary", "body": "Add names, jargon, or acronyms to Dictionary. Add the correct spelling and any variants it tends to mishear."},
        {"emoji": "🔑", "title": "Choose a cleanup engine", "body": "Use Raw to keep the transcript as spoken, Local for on-device cleanup, or configure an optional cloud provider. Cloud modes send transcript text to that provider; audio stays on this device."},
    ];
    tip_rows: list[JsxElement] = [];
    for tip in tips {
        tip_rows.append(<article className="jac-help-card" key={str(tip.get("title", ""))}>
            <span>{str(tip.get("emoji", ""))}</span>
            <div><strong>{str(tip.get("title", ""))}</strong><p>{str(tip.get("body", ""))}</p></div>
        </article>);
    }
    return <section className="jac-help">
        <div className="jac-home-eyebrow">A few tips to get started</div>
        <h1>Help</h1>
        <div className="jac-help-list">{tip_rows}</div>
    </section>;
}

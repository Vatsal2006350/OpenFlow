//! Inspect the raw OpenAI response envelope used by the Jac cleanup bridge.
//! Usage: OPENAI_API_KEY=... cargo run -p openflow-cleanup --example clean

use openflow_cleanup::OpenAiProvider;
use openflow_core::cleanup::CleanupMsg;

fn main() -> anyhow::Result<()> {
    let key = std::env::var("OPENAI_API_KEY")?;
    let provider = OpenAiProvider::new(key);

    let samples = [
        "um so i think we should uh meet at 2 actually 3 tomorrow you know to talk about the the project",
        "can you send the deck over when you get a chance thanks",
    ];
    for raw in samples {
        println!("RAW:     {raw}");
        let messages = [
            CleanupMsg { role: "system", content: "Return a cleaned transcript.".into() },
            CleanupMsg { role: "user", content: raw.to_string() },
        ];
        let request_body = serde_json::json!({
            "model": "gpt-4o-mini",
            "temperature": 0.2,
            "max_tokens": 512,
            "messages": messages.iter().map(|message| serde_json::json!({
                "role": message.role,
                "content": message.content
            })).collect::<Vec<_>>()
        });
        match provider.cleanup_with_request_body(&request_body) {
            Ok(envelope) => println!("RESPONSE ENVELOPE: {envelope}\n"),
            Err(e) => {
                println!("ERROR:   {e}\n");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

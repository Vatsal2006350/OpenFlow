//! Local JSON persistence for records shaped and interpreted by Jac.

use std::path::Path;

use serde_json::Value;

pub fn load(path: &Path) -> Vec<Value> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_str::<Value>(&contents) else {
        return Vec::new();
    };
    file.get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

pub fn save(path: &Path, sessions: &[Value]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = serde_json::json!({"sessions": sessions});
    std::fs::write(path, serde_json::to_vec_pretty(&file)?)
}

#[cfg(test)]
mod tests {
    use super::{load, save};
    use serde_json::json;

    fn temp_path() -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("openflow-stats-{}-{nonce}.json", std::process::id()))
    }

    #[test]
    fn reads_existing_jac_shaped_records_without_a_rust_schema() {
        let path = temp_path();
        let row = json!({
            "ts_unix": 123,
            "words": 4,
            "duration_ms": 2400,
            "chars": 17,
            "text": "Keep the local file",
            "app": "com.example.editor",
            "future_field": {"kept": true}
        });
        std::fs::write(&path, serde_json::to_vec(&json!({"sessions": [row]})).unwrap()).unwrap();

        let records = load(&path);
        let _ = std::fs::remove_file(path);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["words"], 4);
        assert_eq!(records[0]["future_field"]["kept"], true);
    }

    #[test]
    fn saves_the_existing_sessions_envelope() {
        let path = temp_path();
        let sessions = vec![json!({"words": 2, "text": "hello local"})];

        save(&path, &sessions).unwrap();
        let file: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(file["sessions"][0]["text"], "hello local");
    }
}

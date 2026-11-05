//! Local-only Google Calendar read sync for meeting reminders.
//! OAuth refresh tokens live in macOS Keychain; event cache lives in app support.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

const SERVICE: &str = "com.openflow.openflow";
const SCOPE: &str = "openid email https://www.googleapis.com/auth/calendar.readonly";

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub email: String,
    pub connected_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub account_email: String,
    pub join_url: Option<String>,
    #[serde(default)]
    pub cancelled: bool,
}
/// Raw event rows grouped by the account that authorized the calendar read.
/// Provider-to-app field normalization belongs to Jac's calendar policy.
#[derive(Clone, Serialize)]
pub struct CalendarFeed {
    pub account_email: String,
    pub items: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
struct OAuthConfig {
    client_id: String,
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

fn support_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join("Library/Application Support/OpenFlow")
    }
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("OpenFlow")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from(std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            format!("{}/.local/share", std::env::var("HOME").unwrap_or_default())
        }))
        .join("OpenFlow")
    }
}
fn config() -> Result<OAuthConfig, String> {
    serde_json::from_slice(
        &fs::read(support_dir().join("google_calendar_oauth.json"))
            .map_err(|_| "Google Calendar OAuth client is not set up".to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn accounts_path() -> PathBuf {
    support_dir().join("google_calendar_accounts.json")
}
fn events_path() -> PathBuf {
    support_dir().join("google_calendar_events.json")
}
fn accounts() -> Vec<Account> {
    fs::read(accounts_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
fn save_accounts(a: &[Account]) -> Result<(), String> {
    fs::create_dir_all(support_dir()).map_err(|e| e.to_string())?;
    fs::write(
        accounts_path(),
        serde_json::to_vec_pretty(a).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
pub fn list_accounts() -> Vec<Account> {
    accounts()
}
pub fn upcoming() -> Vec<CalendarEvent> {
    fs::read(events_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn save_upcoming(events: &[CalendarEvent]) -> Result<(), String> {
    fs::create_dir_all(support_dir()).map_err(|e| e.to_string())?;
    fs::write(events_path(), serde_json::to_vec_pretty(events).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn key_account(email: &str) -> String {
    // Stable opaque suffix for the Keychain account name; the email remains in
    // the local account index so the refresh token itself contains no PII.
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(email.as_bytes());
    format!(
        "google_calendar_refresh_{}",
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
fn keyring(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
}

pub fn connect(app: AppHandle) {
    thread::spawn(move || {
        let result = oauth_connect();
        let payload = match &result {
            Ok(account) => serde_json::json!({"state":"connected","email":account.email}),
            Err(e) => serde_json::json!({"state":"error","message":e}),
        };
        let _ = app.emit("openflow://calendar/status", payload);
        if let Err(e) = result {
            eprintln!("[openflow] Google Calendar connection failed: {e}");
        }
    });
}

fn oauth_connect() -> Result<Account, String> {
    let cfg = config()?;
    let client_secret = keyring("google_calendar_client_secret")?
        .get_password()
        .map_err(|_| "Google Calendar client secret is missing from Keychain".to_string())?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    listener.set_nonblocking(false).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    use rand::RngCore;
    let mut nonce = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut nonce);
    let state = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|e| e.to_string())?;
    for (k, v) in [
        ("client_id", cfg.client_id.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("access_type", "offline"),
        ("prompt", "consent select_account"),
        ("include_granted_scopes", "true"),
        ("state", state.as_str()),
    ] {
        url.query_pairs_mut().append_pair(k, v);
    }
    #[cfg(target_os = "macos")]
    Command::new("open")
        .arg(url.as_str())
        .spawn()
        .map_err(|e| format!("Could not open sign-in page: {e}"))?;
    #[cfg(target_os = "windows")]
    Command::new("cmd")
        .args(["/C", "start", url.as_str()])
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    Command::new("xdg-open")
        .arg(url.as_str())
        .spawn()
        .map_err(|e| e.to_string())?;
    let (mut stream, _) = listener.accept().map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(180)))
        .map_err(|e| e.to_string())?;
    let mut request = [0u8; 8192];
    let n = stream.read(&mut request).map_err(|e| e.to_string())?;
    let first = String::from_utf8_lossy(&request[..n])
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    let path = first
        .split_whitespace()
        .nth(1)
        .ok_or("Invalid OAuth callback")?;
    let callback =
        reqwest::Url::parse(&format!("http://localhost{path}")).map_err(|e| e.to_string())?;
    let param = |name: &str| {
        callback
            .query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    if param("state").as_deref() != Some(state.as_str()) {
        return Err("OAuth state did not match".into());
    }
    if let Some(err) = param("error") {
        return Err(format!("Google denied access: {err}"));
    }
    let code = param("code").ok_or("Google callback did not include an authorization code")?;
    let response="HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><title>OpenFlow connected</title><h2>Calendar connected. You can return to OpenFlow.</h2>";
    let _ = stream.write_all(response.as_bytes());
    let http = reqwest::blocking::Client::new();
    let tokens: TokenResponse = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("Google token exchange failed: {e}"))?
        .json()
        .map_err(|e| e.to_string())?;
    let email_value: serde_json::Value = http
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(&tokens.access_token)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let email = email_value
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or("Google did not return the account email")?
        .to_lowercase();
    if let Some(refresh) = tokens.refresh_token {
        keyring(&key_account(&email))?
            .set_password(&refresh)
            .map_err(|e| e.to_string())?;
    } else if keyring(&key_account(&email))?.get_password().is_err() {
        return Err("Google did not issue offline access. Retry the account connection and approve the requested access.".into());
    }
    let mut all = accounts();
    if !all.iter().any(|a| a.email == email) {
        all.push(Account {
            email: email.clone(),
            connected_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });
    }
    save_accounts(&all)?;
    Ok(all.into_iter().find(|a| a.email == email).unwrap())
}

fn access_token(email: &str) -> Result<String, String> {
    let refresh = keyring(&key_account(email))?
        .get_password()
        .map_err(|_| format!("No local refresh token for {email}"))?;
    let cfg = config()?;
    let secret = keyring("google_calendar_client_secret")?
        .get_password()
        .map_err(|e| e.to_string())?;
    let v: TokenResponse = reqwest::blocking::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", secret.as_str()),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("Google token refresh failed: {e}"))?
        .json()
        .map_err(|e| e.to_string())?;
    let _ = v.expires_in;
    Ok(v.access_token)
}

fn fetch_events(account: &Account, time_min: &str, time_max: &str) -> Result<CalendarFeed, String> {
    let token = access_token(&account.email)?;
    let value: serde_json::Value = reqwest::blocking::Client::new()
        .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
        .bearer_auth(token)
        .query(&[
            ("timeMin", time_min.to_string()),
            ("timeMax", time_max.to_string()),
            ("singleEvents", "true".into()),
            ("orderBy", "startTime".into()),
            ("maxResults", "250".into()),
        ])
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("Calendar read failed for {}: {e}", account.email))?
        .json()
        .map_err(|e| e.to_string())?;
    Ok(CalendarFeed {
        account_email: account.email.clone(),
        items: value
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
    })
}

pub fn fetch_upcoming(time_min: String, time_max: String) -> Vec<CalendarFeed> {
    let mut all = Vec::new();
    for account in accounts() {
        match fetch_events(&account, &time_min, &time_max) {
            Ok(feed) => all.push(feed),
            Err(e) => eprintln!("[openflow] Calendar sync: {e}"),
        }
    }
    all
}

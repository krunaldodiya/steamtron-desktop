//! SteamTron account authentication, compatible with the existing token file.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::form_urlencoded;

const CLIENT_ID: &str = "umutron-desktop";
const DEFAULT_API: &str = "https://umutron-fastapi.vercel.app";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("SteamTron API returned HTTP {0}")]
    Http(u16),
    #[error("Could not reach SteamTron API: {0}")]
    Network(String),
    #[error("SteamTron API returned invalid JSON")]
    Json,
    #[error("Sign-in timed out. Try again.")]
    Timeout,
    #[error("Sign-in did not return an authorization code.")]
    MissingCode,
    #[error("SteamTron did not return an access token.")]
    MissingToken,
    #[error("{0}")]
    Message(String),
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthState {
    pub authenticated: bool,
    pub base_url: String,
    pub email: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    access_token: String,
    expires_at: i64,
    #[serde(default)]
    email: String,
    #[serde(default)]
    api_base: String,
}

fn data_home() -> PathBuf {
    if let Some(value) = std::env::var_os("STEAMTRON_DATA_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(value) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    if let Some(value) = std::env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share")
}

fn config_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(value) = std::env::var_os("APPDATA") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
}

fn token_path() -> PathBuf {
    data_home().join("steamtron/api-token.json")
}

pub fn api_base() -> String {
    if let Ok(value) = std::env::var("STEAMTRON_API_BASE") {
        if !value.trim().is_empty() {
            return value.trim().trim_end_matches('/').to_owned();
        }
    }
    let path = config_home().join("steamtron/config.json");
    if let Ok(text) = fs::read_to_string(path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(value) = value.get("api_base").and_then(|value| value.as_str()) {
                if !value.trim().is_empty() {
                    return value.trim().trim_end_matches('/').to_owned();
                }
            }
        }
    }
    DEFAULT_API.to_owned()
}

fn key(base: &str) -> String {
    base.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn load_tokens() -> BTreeMap<String, Entry> {
    fs::read_to_string(token_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_tokens(tokens: &BTreeMap<String, Entry>) -> Result<(), AuthError> {
    let path = token_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| AuthError::Io {
            path: parent.to_owned(),
            source: error,
        })?;
    }
    let temporary = path.with_extension("tmp");
    let text = serde_json::to_string_pretty(tokens).map_err(|_| AuthError::Json)?;
    fs::write(&temporary, text).map_err(|error| AuthError::Io {
        path: temporary.clone(),
        source: error,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&temporary, &path).map_err(|error| AuthError::Io {
        path,
        source: error,
    })
}

fn valid(entry: &Entry) -> bool {
    !entry.access_token.is_empty()
        && entry.expires_at > now() + 30
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn access_token() -> String {
    let base = api_base();
    load_tokens()
        .remove(&key(&base))
        .filter(valid)
        .map(|entry| entry.access_token)
        .unwrap_or_default()
}

pub fn status() -> AuthState {
    let base = api_base();
    let entry = load_tokens().remove(&key(&base));
    match entry {
        Some(entry) if valid(&entry) => AuthState {
            authenticated: true,
            base_url: base,
            email: entry.email,
            expires_at: entry.expires_at,
        },
        Some(entry) => AuthState {
            authenticated: false,
            base_url: base,
            email: String::new(),
            expires_at: entry.expires_at,
        },
        None => AuthState {
            authenticated: false,
            base_url: base,
            email: String::new(),
            expires_at: 0,
        },
    }
}

fn pkce() -> (String, String) {
    let verifier: String = rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();
    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    (verifier, challenge)
}

fn request_json(request: reqwest::blocking::RequestBuilder) -> Result<serde_json::Value, AuthError> {
    let response = request.send().map_err(|error| AuthError::Network(error.to_string()))?;
    let status = response.status().as_u16();
    let text = response.text().map_err(|error| AuthError::Network(error.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(AuthError::Http(status));
    }
    serde_json::from_str(&text).map_err(|_| AuthError::Json)
}


/// Open the OAuth URL with the system default browser.
fn open_in_browser(url: &str) -> Result<(), String> {
    webbrowser::open(url).map_err(|error| error.to_string())
}

pub fn login() -> Result<AuthState, AuthError> {
    let base = api_base();
    let current = status();
    if current.authenticated {
        return Ok(current);
    }
    let (verifier, challenge) = pkce();
    let state: String = rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| AuthError::Network(error.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|error| AuthError::Network(error.to_string()))?
        .port();
    listener
        .set_nonblocking(true)
        .map_err(|error| AuthError::Network(error.to_string()))?;
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .finish();
    let url = format!("{base}/oauth/authorize?{query}");
    if let Err(error) = open_in_browser(&url) {
        return Err(AuthError::Message(format!(
            "Open this address to sign in: {url} ({error})"
        )));
    }

    let start = Instant::now();
    let code = loop {
        if start.elapsed() > SIGN_IN_TIMEOUT {
            return Err(AuthError::Message(format!(
                "Sign-in timed out. Open this address to sign in: {url}"
            )));
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buffer = [0u8; 8192];
                let count = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..count]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let params: BTreeMap<String, String> = path
                    .split_once('?')
                    .map(|(_, query)| {
                        form_urlencoded::parse(query.as_bytes())
                            .into_owned()
                            .collect()
                    })
                    .unwrap_or_default();
                let body = b"<html><body><h2>SteamTron</h2><p>Sign-in complete. Close this window.</p></body></html>";
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                );
                let _ = stream.write_all(body);
                if params.get("state") != Some(&state) {
                    continue;
                }
                if let Some(error) = params.get("error") {
                    return Err(AuthError::Message(format!("Sign-in refused: {error}")));
                }
                if let Some(code) = params.get("code") {
                    break code.clone();
                }
                return Err(AuthError::MissingCode);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(AuthError::Network(error.to_string())),
        }
    };

    let form = form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", &code)
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("code_verifier", &verifier)
        .finish();
    let client = reqwest::blocking::Client::new();
    let payload = request_json(
        client
            .post(format!("{base}/oauth/token"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form),
    )?;
    let token = payload
        .get("access_token")
        .and_then(|value| value.as_str())
        .ok_or(AuthError::MissingToken)?;
    let expires_in = payload
        .get("expires_in")
        .and_then(|value| value.as_i64())
        .unwrap_or(3600);
    let email = payload
        .get("user")
        .and_then(|value| value.get("email"))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned();
    let expires_at = now() + expires_in;
    let mut tokens = load_tokens();
    tokens.insert(
        key(&base),
        Entry {
            access_token: token.to_owned(),
            expires_at,
            email: email.clone(),
            api_base: base.clone(),
        },
    );
    save_tokens(&tokens)?;
    Ok(AuthState {
        authenticated: true,
        base_url: base,
        email,
        expires_at,
    })
}

pub fn logout() -> AuthState {
    let base = api_base();
    let mut tokens = load_tokens();
    tokens.remove(&key(&base));
    let _ = save_tokens(&tokens);
    AuthState {
        authenticated: false,
        base_url: base,
        email: String::new(),
        expires_at: 0,
    }
}

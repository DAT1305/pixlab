#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use reqwest::blocking::Client;
use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONNECTION, CONTENT_TYPE,
    USER_AGENT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::Manager;
use time::{format_description::well_known::Rfc3339, Duration as TimeDuration, OffsetDateTime};
use tiny_http::{Response, Server, StatusCode};

const CODEX_AUTH_URL: &str = "https://auth.openai.com/oauth/authorize";
const CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const CODEX_CALLBACK_PORT: u16 = 1455;
const CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const CODEX_CLIENT_VERSION: &str = "0.101.0";
const CODEX_USER_AGENT: &str = "codex_cli_rs/0.101.0 (Mac OS 26.0.1; arm64) Apple_Terminal/464";
const CODEX_IMAGE_MODEL: &str = "gpt-5.4";
const CODEX_REFERENCE_IMAGE_MAX_DATA_URL_BYTES: usize = 24 * 1024 * 1024;

#[derive(Default)]
struct CodexState {
    lock: Arc<Mutex<()>>,
    logins: Arc<Mutex<HashMap<String, LoginProgress>>>,
    active_login: Arc<Mutex<Option<ActiveLogin>>>,
}

#[derive(Debug, Clone, Default)]
struct LoginProgress {
    done: bool,
    success: bool,
    email: Option<String>,
    error: Option<String>,
}

#[derive(Clone)]
struct ActiveLogin {
    state: String,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct CodexTokenStorage {
    id_token: String,
    access_token: String,
    refresh_token: String,
    account_id: String,
    last_refresh: String,
    email: String,
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    expired: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexStatus {
    authenticated: bool,
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexGeneratePayload {
    prompt: String,
    #[serde(default)]
    reference_image_data_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexLoginStatusPayload {
    login_state: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CodexGenerateResult {
    image_path: String,
    file_name: String,
    image_data_url: String,
    revised_prompt: Option<String>,
}

#[derive(Debug, Clone)]
struct CodexGeneratedImage {
    png_base64: String,
    revised_prompt: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexLoginStartResult {
    state: String,
    auth_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexLoginPollResult {
    done: bool,
    success: bool,
    email: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenExternalUrlPayload {
    url: String,
}

#[derive(Debug)]
struct OAuthCallback {
    code: String,
    state: String,
    error: String,
}

#[derive(Debug)]
struct PkceCodes {
    code_verifier: String,
    code_challenge: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: String,
    expires_in: i64,
}

#[derive(Debug, Default)]
struct JwtProfile {
    email: String,
    account_id: String,
}

fn app_token_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Failed to resolve app data directory: {error}"))?;
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Failed to create app data directory: {error}"))?;
    Ok(dir.join("codex-auth.json"))
}

fn load_token(app: &tauri::AppHandle) -> Result<Option<CodexTokenStorage>, String> {
    let path = app_token_path(app)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read Codex auth file: {error}"))?;
    let token: CodexTokenStorage = serde_json::from_str(&raw)
        .map_err(|error| format!("Failed to parse Codex auth file: {error}"))?;
    Ok(Some(token))
}

fn save_token(app: &tauri::AppHandle, token: &CodexTokenStorage) -> Result<(), String> {
    let path = app_token_path(app)?;
    let raw = serde_json::to_string_pretty(token)
        .map_err(|error| format!("Failed to serialize Codex auth file: {error}"))?;
    fs::write(path, raw).map_err(|error| format!("Failed to save Codex auth file: {error}"))
}

fn random_urlsafe(bytes_len: usize) -> String {
    let mut bytes = vec![0_u8; bytes_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn random_hex(bytes_len: usize) -> String {
    let mut bytes = vec![0_u8; bytes_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn generate_pkce_codes() -> PkceCodes {
    let code_verifier = random_urlsafe(96);
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

fn build_auth_url(state: &str, pkce: &PkceCodes) -> String {
    let params = [
        ("client_id", CODEX_CLIENT_ID),
        ("response_type", "code"),
        ("redirect_uri", CODEX_REDIRECT_URI),
        ("scope", "openid email profile offline_access"),
        ("state", state),
        ("code_challenge", pkce.code_challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("prompt", "login"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
    ];
    let query = params
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{CODEX_AUTH_URL}?{query}")
}

fn callback_success_html() -> String {
    "<html><head><meta charset=\"utf-8\"><title>Authentication successful</title><script>setTimeout(function(){window.close();},1200);</script></head><body style=\"font-family:sans-serif;padding:24px;background:#111;color:#eee\"><h1>Authentication successful</h1><p>You can return to PixLab Desktop.</p></body></html>".to_string()
}

fn callback_error_html(message: &str) -> String {
    format!(
        "<html><head><meta charset=\"utf-8\"><title>Authentication failed</title></head><body style=\"font-family:sans-serif;padding:24px;background:#111;color:#eee\"><h1>Authentication failed</h1><p>{}</p></body></html>",
        message
    )
}

fn start_oauth_callback_server(
    cancel: Arc<AtomicBool>,
) -> Result<mpsc::Receiver<Result<OAuthCallback, String>>, String> {
    let server = Server::http(("127.0.0.1", CODEX_CALLBACK_PORT))
        .map_err(|error| {
            format!(
                "Port {CODEX_CALLBACK_PORT} is already in use. Close Codex Manager or any app holding localhost:{CODEX_CALLBACK_PORT}, then try again. Original error: {error}"
            )
        })?;
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let request = loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = tx.send(Err("Codex login cancelled.".to_string()));
                return;
            }
            match server.recv_timeout(Duration::from_secs(1)) {
                Ok(Some(request)) => break request,
                Ok(None) => continue,
                Err(error) => {
                    let _ = tx.send(Err(format!(
                        "Failed while waiting for Codex callback: {error}"
                    )));
                    return;
                }
            }
        };

        let full_url = format!("http://localhost:{CODEX_CALLBACK_PORT}{}", request.url());
        let parsed = reqwest::Url::parse(&full_url)
            .map_err(|error| format!("Invalid callback URL: {error}"));

        match parsed {
            Ok(url) => {
                let params = url.query_pairs().collect::<Vec<_>>();
                let code = params
                    .iter()
                    .find(|(key, _)| key == "code")
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_default();
                let state = params
                    .iter()
                    .find(|(key, _)| key == "state")
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_default();
                let error_param = params
                    .iter()
                    .find(|(key, _)| key == "error")
                    .map(|(_, value)| value.to_string())
                    .unwrap_or_default();

                let (html, status_code) = if !error_param.is_empty() {
                    (callback_error_html(&error_param), StatusCode(400))
                } else if code.is_empty() || state.is_empty() {
                    (
                        callback_error_html("Missing code or state."),
                        StatusCode(400),
                    )
                } else {
                    (callback_success_html(), StatusCode(200))
                };

                let response = Response::from_string(html)
                    .with_status_code(status_code)
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8")
                            .expect("static header"),
                    );
                let _ = request.respond(response);

                let _ = tx.send(Ok(OAuthCallback {
                    code,
                    state,
                    error: error_param,
                }));
            }
            Err(error) => {
                let response = Response::from_string(callback_error_html(&error))
                    .with_status_code(StatusCode(400))
                    .with_header(
                        tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8")
                            .expect("static header"),
                    );
                let _ = request.respond(response);
                let _ = tx.send(Err(error));
            }
        }
    });

    Ok(rx)
}

fn open_external_url(url: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err("Missing URL to open.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|error| format!("Failed to open browser: {error}"))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|error| format!("Failed to open browser: {error}"))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| format!("Failed to open browser: {error}"))?;
        return Ok(());
    }
}

fn parse_jwt_profile(id_token: &str) -> JwtProfile {
    let payload = id_token.split('.').nth(1).unwrap_or_default();
    if payload.is_empty() {
        return JwtProfile::default();
    }

    let decoded = match URL_SAFE_NO_PAD.decode(payload.as_bytes()) {
        Ok(decoded) => decoded,
        Err(_) => return JwtProfile::default(),
    };

    let value: Value = match serde_json::from_slice(&decoded) {
        Ok(value) => value,
        Err(_) => return JwtProfile::default(),
    };

    JwtProfile {
        email: value
            .get("email")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        account_id: value
            .get("https://api.openai.com/auth")
            .and_then(|auth| auth.get("chatgpt_account_id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn unix_now_rfc3339(seconds_from_now: i64) -> String {
    (OffsetDateTime::now_utc() + TimeDuration::seconds(seconds_from_now))
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn current_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn exchange_code_for_tokens(
    client: &Client,
    code: &str,
    pkce: &PkceCodes,
) -> Result<CodexTokenStorage, String> {
    let response = client
        .post(CODEX_TOKEN_URL)
        .header(ACCEPT, "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CODEX_CLIENT_ID),
            ("code", code),
            ("redirect_uri", CODEX_REDIRECT_URI),
            ("code_verifier", pkce.code_verifier.as_str()),
        ])
        .send()
        .map_err(|error| format!("Token exchange request failed: {error}"))?;

    let status = response.status();
    let token_response: TokenResponse = if status.is_success() {
        response
            .json()
            .map_err(|error| format!("Failed to parse token exchange response: {error}"))?
    } else {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "Token exchange failed with status {}: {}",
            status.as_u16(),
            body
        ));
    };

    let profile = parse_jwt_profile(&token_response.id_token);
    Ok(CodexTokenStorage {
        id_token: token_response.id_token,
        access_token: token_response.access_token,
        refresh_token: token_response.refresh_token.unwrap_or_default(),
        account_id: profile.account_id,
        last_refresh: now_rfc3339(),
        email: profile.email,
        r#type: "codex".to_string(),
        expired: unix_now_rfc3339(token_response.expires_in),
    })
}

fn refresh_tokens(client: &Client, token: &CodexTokenStorage) -> Result<CodexTokenStorage, String> {
    if token.refresh_token.trim().is_empty() {
        return Err("Codex refresh token is missing.".to_string());
    }

    let response = client
        .post(CODEX_TOKEN_URL)
        .header(ACCEPT, "application/json")
        .form(&[
            ("client_id", CODEX_CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", token.refresh_token.as_str()),
            ("scope", "openid profile email"),
        ])
        .send()
        .map_err(|error| format!("Token refresh request failed: {error}"))?;

    let status = response.status();
    let token_response: TokenResponse = if status.is_success() {
        response
            .json()
            .map_err(|error| format!("Failed to parse refresh response: {error}"))?
    } else {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "Token refresh failed with status {}: {}",
            status.as_u16(),
            body
        ));
    };

    let profile = parse_jwt_profile(&token_response.id_token);
    Ok(CodexTokenStorage {
        id_token: token_response.id_token,
        access_token: token_response.access_token,
        refresh_token: token_response
            .refresh_token
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| token.refresh_token.clone()),
        account_id: if profile.account_id.is_empty() {
            token.account_id.clone()
        } else {
            profile.account_id
        },
        last_refresh: now_rfc3339(),
        email: if profile.email.is_empty() {
            token.email.clone()
        } else {
            profile.email
        },
        r#type: "codex".to_string(),
        expired: unix_now_rfc3339(token_response.expires_in),
    })
}

fn needs_refresh(token: &CodexTokenStorage) -> bool {
    if token.access_token.trim().is_empty() {
        return true;
    }
    let parsed = OffsetDateTime::parse(&token.expired, &Rfc3339);
    match parsed {
        Ok(expiry) => expiry.unix_timestamp() <= current_unix() + 60,
        Err(_) => true,
    }
}

fn ensure_fresh_token(
    app: &tauri::AppHandle,
    client: &Client,
    token: CodexTokenStorage,
) -> Result<CodexTokenStorage, String> {
    if !needs_refresh(&token) {
        return Ok(token);
    }
    let refreshed = refresh_tokens(client, &token)?;
    save_token(app, &refreshed)?;
    Ok(refreshed)
}

fn build_codex_headers(token: &CodexTokenStorage, stream: bool) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token.access_token))
            .map_err(|error| format!("Invalid authorization header: {error}"))?,
    );
    headers.insert("version", HeaderValue::from_static(CODEX_CLIENT_VERSION));
    headers.insert(
        "session_id",
        HeaderValue::from_str(&random_hex(16))
            .map_err(|error| format!("Invalid session header: {error}"))?,
    );
    headers.insert(USER_AGENT, HeaderValue::from_static(CODEX_USER_AGENT));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(if stream {
            "text/event-stream"
        } else {
            "application/json"
        }),
    );
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    headers.insert(CONNECTION, HeaderValue::from_static("Keep-Alive"));
    headers.insert("originator", HeaderValue::from_static("codex_cli_rs"));
    if !token.account_id.trim().is_empty() {
        headers.insert(
            "chatgpt-account-id",
            HeaderValue::from_str(&token.account_id)
                .map_err(|error| format!("Invalid Chatgpt-Account-Id header: {error}"))?,
        );
    }
    Ok(headers)
}

fn build_spritesheet_prompt(user_prompt: &str, has_reference_image: bool) -> String {
    let reference_requirements = if has_reference_image {
        "\n- use the provided input image only as the character design reference\n- preserve the same character identity, silhouette, proportions, outfit, colors, hairstyle, and carried items from the reference image\n- generate the requested animation frames as that same character; do not copy the reference background, pose layout, lighting, shadows, or image crop\n- if the reference character has a weapon or prop, keep it as part of the character body motion only"
    } else {
        ""
    };

    format!(
        "Create a clean 2D pixel-art animation spritesheet for this request: {user_prompt}\n\nRequirements:{reference_requirements}\n- output exactly one spritesheet image, not multiple separate images\n- arrange the sheet as a strict grid with 6 frames per row\n- each row is exactly one animation sequence\n- use at most 6 rows total, so the maximum sheet is 6 x 6 frames: up to 6 animations with 6 frames each\n- if multiple animations are requested, put each animation on its own row; never wrap one animation across multiple rows\n- do not add extra rows, duplicate rows, filler rows, labels, or separators\n- show only the character and the raw body/weapon motion\n- do not include any action VFX or secondary effects: no muzzle flashes, bullets, projectiles, slash trails, impact sparks, hit flashes, smoke, dust, glow, particles, speed lines, magic effects, or motion smear\n- if the prompt describes shooting, swinging, casting, or attacking, animate only the character pose and held item; omit the emitted effect entirely\n- keep all frames aligned to a consistent baseline\n- use a flat solid bright magenta background (#ff00ff) only\n- keep even spacing and clean cell boundaries\n- no UI, no text, no watermark, no border\n- keep the whole character visible in every frame\n- style should be game-ready pixel art"
    )
}

fn build_codex_request(prompt: &str, reference_image_data_url: Option<&str>) -> Value {
    let mut content = vec![json!({
        "type": "input_text",
        "text": build_spritesheet_prompt(prompt, reference_image_data_url.is_some())
    })];
    if let Some(image_url) = reference_image_data_url {
        content.push(json!({
            "type": "input_image",
            "image_url": image_url
        }));
    }

    json!({
        "model": CODEX_IMAGE_MODEL,
        "stream": true,
        "store": false,
        "parallel_tool_calls": true,
        "include": ["reasoning.encrypted_content"],
        "instructions": "",
        "input": [{
            "type": "message",
            "role": "user",
            "content": content
        }],
        "tools": [{
            "type": "image_generation",
            "size": "auto",
            "partial_images": 2
        }]
    })
}

fn sanitize_reference_image_data_url(value: Option<&String>) -> Result<Option<String>, String> {
    let Some(data_url) = value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    if data_url.len() > CODEX_REFERENCE_IMAGE_MAX_DATA_URL_BYTES {
        return Err("Reference image is too large.".to_string());
    }

    let header = data_url
        .split_once(',')
        .map(|(header, _)| header.to_ascii_lowercase())
        .ok_or_else(|| "Reference image must be a base64 image data URL.".to_string())?;
    let supported_mime = [
        "data:image/png",
        "data:image/jpeg",
        "data:image/jpg",
        "data:image/webp",
        "data:image/gif",
    ];
    if !supported_mime
        .iter()
        .any(|prefix| header.starts_with(prefix))
        || !header.contains(";base64")
    {
        return Err("Reference image must be a PNG, JPG, WEBP, or GIF data URL.".to_string());
    }

    Ok(Some(data_url.to_string()))
}

fn request_codex_spritesheet(
    app: &tauri::AppHandle,
    client: &Client,
    payload: &CodexGeneratePayload,
) -> Result<CodexGeneratedImage, String> {
    let stored = load_token(app)?.ok_or_else(|| "Codex is not logged in.".to_string())?;
    let token = ensure_fresh_token(app, client, stored)?;
    let reference_image_data_url =
        sanitize_reference_image_data_url(payload.reference_image_data_url.as_ref())?;
    let request_body = build_codex_request(&payload.prompt, reference_image_data_url.as_deref());
    match request_codex_spritesheet_via_curl(&token, &request_body) {
        Ok(result) => Ok(result),
        Err(curl_error) => {
            let headers = build_codex_headers(&token, true)?;
            let response = client
                .post(format!("{CODEX_BASE_URL}/responses"))
                .headers(headers)
                .json(&request_body)
                .send()
                .map_err(|error| {
                    format!(
                        "Codex request failed after curl fallback error ({curl_error}): {error}"
                    )
                })?;

            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                let refreshed = refresh_tokens(client, &token)?;
                save_token(app, &refreshed)?;
                match request_codex_spritesheet_via_curl(&refreshed, &request_body) {
                    Ok(result) => return Ok(result),
                    Err(retry_curl_error) => {
                        let retry_headers = build_codex_headers(&refreshed, true)?;
                        let retry_response = client
                            .post(format!("{CODEX_BASE_URL}/responses"))
                            .headers(retry_headers)
                            .json(&request_body)
                            .send()
                            .map_err(|error| format!("Codex retry failed after curl fallback error ({retry_curl_error}): {error}"))?;
                        return parse_codex_stream_response(retry_response).map_err(|error| {
                            format!("{error}; curl retry also failed: {retry_curl_error}")
                        });
                    }
                }
            }

            parse_codex_stream_response(response)
                .map_err(|error| format!("{error}; curl fallback also failed: {curl_error}"))
        }
    }
}

fn request_codex_spritesheet_via_curl(
    token: &CodexTokenStorage,
    request_body: &Value,
) -> Result<CodexGeneratedImage, String> {
    let request_json = serde_json::to_vec(request_body)
        .map_err(|error| format!("Failed to encode Codex request: {error}"))?;
    let request_url = format!("{CODEX_BASE_URL}/responses");
    let mut child = Command::new("curl")
        .arg("--http1.1")
        .arg("-sS")
        .arg("--fail-with-body")
        .arg(&request_url)
        .arg("-H")
        .arg("content-type: application/json")
        .arg("-H")
        .arg(format!("authorization: Bearer {}", token.access_token))
        .arg("-H")
        .arg(format!("version: {}", CODEX_CLIENT_VERSION))
        .arg("-H")
        .arg(format!("session_id: {}", random_hex(16)))
        .arg("-H")
        .arg(format!("user-agent: {}", CODEX_USER_AGENT))
        .arg("-H")
        .arg("accept: text/event-stream")
        .arg("-H")
        .arg("accept-encoding: identity")
        .arg("-H")
        .arg("connection: Keep-Alive")
        .arg("-H")
        .arg("originator: codex_cli_rs")
        .arg("-H")
        .arg(format!("chatgpt-account-id: {}", token.account_id))
        .arg("--data-binary")
        .arg("@-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to start curl for Codex request: {error}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(&request_json)
            .map_err(|error| format!("Failed to write Codex request into curl stdin: {error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("Failed to wait for curl Codex request: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "curl Codex request failed (status {:?}): {} {}",
            output.status.code(),
            stderr.trim(),
            stdout.trim()
        ));
    }

    let body = String::from_utf8_lossy(&output.stdout);
    parse_codex_sse_text(&body)
}

fn extract_codex_error_message(value: &Value) -> Option<String> {
    fn read_error(error: &Value) -> Option<String> {
        if let Some(message) = error.as_str() {
            return Some(message.to_string());
        }
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| error.get("code").and_then(Value::as_str))
            .or_else(|| error.get("type").and_then(Value::as_str));
        message.map(|entry| entry.to_string())
    }

    value
        .get("error")
        .and_then(read_error)
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("error"))
                .and_then(read_error)
        })
        .or_else(|| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(|entry| entry.to_string())
        })
}

fn extract_generated_image_from_item(item: &Value) -> Option<CodexGeneratedImage> {
    if item.get("type").and_then(Value::as_str) != Some("image_generation_call") {
        return None;
    }

    let result = item
        .get("result")
        .and_then(Value::as_str)
        .or_else(|| item.get("partial_image_b64").and_then(Value::as_str))
        .unwrap_or_default()
        .trim()
        .to_string();
    if result.is_empty() {
        return None;
    }

    Some(CodexGeneratedImage {
        png_base64: result,
        revised_prompt: item
            .get("revised_prompt")
            .and_then(Value::as_str)
            .map(|entry| entry.to_string()),
    })
}

fn codex_body_preview(body: &str) -> String {
    let mut events: Vec<String> = Vec::new();
    let mut snippets: Vec<String> = Vec::new();

    for raw_line in body.lines().take(80) {
        let line = raw_line.trim();
        if line.is_empty() || line == "data: [DONE]" {
            continue;
        }

        let payload = line.strip_prefix("data:").map(str::trim).unwrap_or(line);
        if let Ok(value) = serde_json::from_str::<Value>(payload) {
            if let Some(message) = extract_codex_error_message(&value) {
                return format!("Codex error event: {message}");
            }
            if let Some(event_type) = value.get("type").and_then(Value::as_str) {
                if !events.iter().any(|entry| entry == event_type) {
                    events.push(event_type.to_string());
                }
            }
            continue;
        }

        if payload.contains("partial_image_b64") || payload.contains("\"result\"") {
            continue;
        }
        snippets.push(payload.chars().take(180).collect());
    }

    if !events.is_empty() {
        return format!("seen events: {}", events.join(", "));
    }
    if !snippets.is_empty() {
        return snippets.join(" | ");
    }
    "empty response body".to_string()
}

fn parse_codex_sse_text(body: &str) -> Result<CodexGeneratedImage, String> {
    let mut partial_images: std::collections::BTreeMap<i64, CodexGeneratedImage> =
        std::collections::BTreeMap::new();
    let mut seen_events: Vec<String> = Vec::new();

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if !line.starts_with("data:") {
            continue;
        }
        let payload = line.trim_start_matches("data:").trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }

        let value: Value = serde_json::from_str(payload)
            .map_err(|error| format!("Failed to parse Codex event stream: {error}"))?;
        let event_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !event_type.is_empty() && !seen_events.iter().any(|entry| entry == event_type) {
            seen_events.push(event_type.to_string());
        }

        if event_type == "response.failed" || event_type == "error" {
            return Err(format!(
                "Codex failed to generate the spritesheet: {}",
                extract_codex_error_message(&value).unwrap_or_else(|| "unknown error".to_string())
            ));
        }

        if event_type == "response.image_generation_call.partial_image" {
            let result = value
                .get("partial_image_b64")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            if result.is_empty() {
                continue;
            }
            let output_index = value
                .get("output_index")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            partial_images.insert(
                output_index,
                CodexGeneratedImage {
                    png_base64: result,
                    revised_prompt: value
                        .get("revised_prompt")
                        .and_then(Value::as_str)
                        .map(|entry| entry.to_string()),
                },
            );
            continue;
        }

        if event_type == "response.output_item.done" {
            if let Some(result) = value
                .get("item")
                .and_then(extract_generated_image_from_item)
            {
                return Ok(result);
            }
            continue;
        }

        if event_type == "response.completed" {
            if let Some(error) = extract_codex_error_message(&value) {
                return Err(format!("Codex failed to generate the spritesheet: {error}"));
            }
            let outputs = value
                .get("response")
                .and_then(|response| response.get("output"))
                .and_then(Value::as_array);
            if let Some(outputs) = outputs {
                for item in outputs {
                    if let Some(result) = extract_generated_image_from_item(item) {
                        return Ok(result);
                    }
                }
            }
            if let Some(result) = partial_images.values().next().cloned() {
                return Ok(result);
            }
        }
    }

    let details = if seen_events.is_empty() {
        codex_body_preview(body)
    } else {
        format!("seen events: {}", seen_events.join(", "))
    };
    Err(format!(
        "Codex did not return a spritesheet image ({details})."
    ))
}

fn format_error_chain(error: &dyn StdError) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = error.source();
    while let Some(entry) = source {
        let message = entry.to_string();
        if !parts.iter().any(|part| part == &message) {
            parts.push(message);
        }
        source = entry.source();
    }
    parts.join(": ")
}

fn parse_codex_stream_response(
    response: reqwest::blocking::Response,
) -> Result<CodexGeneratedImage, String> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "Codex image generation failed with status {}: {}",
            status.as_u16(),
            body
        ));
    }
    let mut response = response;
    let mut body = String::new();
    match response.read_to_string(&mut body) {
        Ok(_) => parse_codex_sse_text(&body),
        Err(error) => {
            if body.trim().is_empty() {
                return Err(format!(
                    "Failed to read Codex response: {}",
                    format_error_chain(&error)
                ));
            }
            parse_codex_sse_text(&body).map_err(|parse_error| {
                format!(
                    "Failed to read full Codex response: {}; partial response parse failed: {parse_error}; {}",
                    format_error_chain(&error),
                    codex_body_preview(&body)
                )
            })
        }
    }
}

fn persist_generated_spritesheet(
    app: &tauri::AppHandle,
    image: &CodexGeneratedImage,
) -> Result<CodexGenerateResult, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image.png_base64.as_bytes())
        .map_err(|error| format!("Failed to decode generated spritesheet: {error}"))?;
    let base_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Failed to resolve app data directory: {error}"))?;
    let output_dir = base_dir.join("generated");
    fs::create_dir_all(&output_dir)
        .map_err(|error| format!("Failed to create generated image directory: {error}"))?;
    let file_name = format!(
        "codex-spritesheet-{}.png",
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    );
    let image_path = output_dir.join(&file_name);
    fs::write(&image_path, bytes)
        .map_err(|error| format!("Failed to save generated spritesheet: {error}"))?;
    Ok(CodexGenerateResult {
        image_path: image_path.to_string_lossy().to_string(),
        file_name,
        image_data_url: format!("data:image/png;base64,{}", image.png_base64),
        revised_prompt: image.revised_prompt.clone(),
    })
}

#[tauri::command]
fn codex_auth_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, CodexState>,
) -> Result<CodexStatus, String> {
    let _guard = state
        .lock
        .lock()
        .map_err(|_| "Failed to lock Codex session.".to_string())?;
    let client = Client::builder()
        .http1_only()
        .build()
        .map_err(|error| format!("Failed to create HTTP client: {error}"))?;

    let Some(token) = load_token(&app)? else {
        return Ok(CodexStatus {
            authenticated: false,
            email: None,
        });
    };

    let token = ensure_fresh_token(&app, &client, token)?;
    Ok(CodexStatus {
        authenticated: !token.access_token.trim().is_empty(),
        email: if token.email.trim().is_empty() {
            None
        } else {
            Some(token.email)
        },
    })
}

#[tauri::command]
fn codex_login_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, CodexState>,
) -> Result<CodexLoginStartResult, String> {
    let _guard = state
        .lock
        .lock()
        .map_err(|_| "Failed to lock Codex session.".to_string())?;

    if let Ok(mut active) = state.active_login.lock() {
        if let Some(current) = active.take() {
            current.cancel.store(true, Ordering::SeqCst);
        }
    }

    let pkce = generate_pkce_codes();
    let expected_state = random_hex(16);
    let auth_url = build_auth_url(&expected_state, &pkce);
    let cancel = Arc::new(AtomicBool::new(false));
    let callback_rx = start_oauth_callback_server(Arc::clone(&cancel))?;

    {
        let mut logins = state
            .logins
            .lock()
            .map_err(|_| "Failed to store Codex login state.".to_string())?;
        logins.insert(expected_state.clone(), LoginProgress::default());
    }
    {
        let mut active = state
            .active_login
            .lock()
            .map_err(|_| "Failed to store active Codex login.".to_string())?;
        *active = Some(ActiveLogin {
            state: expected_state.clone(),
            cancel: Arc::clone(&cancel),
        });
    }

    let app_handle = app.clone();
    let state_key = expected_state.clone();
    let pkce_bg = pkce;
    let state_store = Arc::clone(&state.logins);
    let active_store = Arc::clone(&state.active_login);
    thread::spawn(move || {
        let client = match Client::builder().http1_only().build() {
            Ok(client) => client,
            Err(error) => {
                if let Ok(mut map) = state_store.lock() {
                    map.insert(
                        state_key.clone(),
                        LoginProgress {
                            done: true,
                            success: false,
                            email: None,
                            error: Some(format!("Failed to create HTTP client: {error}")),
                        },
                    );
                }
                return;
            }
        };

        let callback = match callback_rx.recv() {
            Ok(result) => result,
            Err(error) => Err(format!("Failed to receive OAuth callback: {error}")),
        };

        let progress = match callback {
            Ok(callback) => {
                if !callback.error.trim().is_empty() {
                    LoginProgress {
                        done: true,
                        success: false,
                        email: None,
                        error: Some(format!("Codex authentication failed: {}", callback.error)),
                    }
                } else if callback.state != state_key {
                    LoginProgress {
                        done: true,
                        success: false,
                        email: None,
                        error: Some("Codex authentication failed: state mismatch.".to_string()),
                    }
                } else {
                    match exchange_code_for_tokens(&client, &callback.code, &pkce_bg) {
                        Ok(token) => {
                            let email = if token.email.trim().is_empty() {
                                None
                            } else {
                                Some(token.email.clone())
                            };
                            match save_token(&app_handle, &token) {
                                Ok(_) => LoginProgress {
                                    done: true,
                                    success: true,
                                    email,
                                    error: None,
                                },
                                Err(error) => LoginProgress {
                                    done: true,
                                    success: false,
                                    email: None,
                                    error: Some(error),
                                },
                            }
                        }
                        Err(error) => LoginProgress {
                            done: true,
                            success: false,
                            email: None,
                            error: Some(error),
                        },
                    }
                }
            }
            Err(error) => LoginProgress {
                done: true,
                success: false,
                email: None,
                error: Some(error),
            },
        };

        if let Ok(mut map) = state_store.lock() {
            map.insert(state_key.clone(), progress);
        }
        if let Ok(mut active) = active_store.lock() {
            if active.as_ref().map(|entry| entry.state.as_str()) == Some(state_key.as_str()) {
                *active = None;
            }
        }
    });

    Ok(CodexLoginStartResult {
        state: expected_state,
        auth_url,
    })
}

#[tauri::command]
fn codex_login_cancel(state: tauri::State<'_, CodexState>) -> Result<(), String> {
    let mut active = state
        .active_login
        .lock()
        .map_err(|_| "Failed to cancel Codex login.".to_string())?;
    if let Some(current) = active.take() {
        current.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut logins) = state.logins.lock() {
            logins.insert(
                current.state,
                LoginProgress {
                    done: true,
                    success: false,
                    email: None,
                    error: Some("Codex login cancelled.".to_string()),
                },
            );
        }
    }
    Ok(())
}

#[tauri::command]
fn codex_login_status(
    state: tauri::State<'_, CodexState>,
    payload: CodexLoginStatusPayload,
) -> Result<CodexLoginPollResult, String> {
    let logins = state
        .logins
        .lock()
        .map_err(|_| "Failed to read Codex login state.".to_string())?;
    let progress = logins
        .get(&payload.login_state)
        .cloned()
        .unwrap_or_default();
    Ok(CodexLoginPollResult {
        done: progress.done,
        success: progress.success,
        email: progress.email,
        error: progress.error,
    })
}

#[tauri::command]
fn desktop_open_external_url(payload: OpenExternalUrlPayload) -> Result<(), String> {
    open_external_url(&payload.url)
}

#[tauri::command]
async fn codex_generate_spritesheet(
    app: tauri::AppHandle,
    state: tauri::State<'_, CodexState>,
    payload: CodexGeneratePayload,
) -> Result<CodexGenerateResult, String> {
    if payload.prompt.trim().is_empty() {
        return Err("Prompt is required.".to_string());
    }
    let lock = state.lock.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock
            .lock()
            .map_err(|_| "Failed to lock Codex session.".to_string())?;
        let client = Client::builder()
            .http1_only()
            .build()
            .map_err(|error| format!("Failed to create HTTP client: {error}"))?;
        let image = request_codex_spritesheet(&app, &client, &payload)?;
        persist_generated_spritesheet(&app, &image)
    })
    .await
    .map_err(|error| format!("Codex worker failed: {error}"))?
}

fn main() {
    tauri::Builder::default()
        .manage(CodexState::default())
        .invoke_handler(tauri::generate_handler![
            codex_auth_status,
            codex_login_start,
            codex_login_cancel,
            codex_login_status,
            desktop_open_external_url,
            codex_generate_spritesheet
        ])
        .run(tauri::generate_context!())
        .expect("failed to run PixLab desktop app");
}

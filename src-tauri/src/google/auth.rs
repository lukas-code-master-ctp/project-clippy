//! Conexión con Google: OAuth para apps de escritorio (PKCE + redirección a 127.0.0.1).
//!
//! El navegador del usuario muestra la pantalla de Google; Brote solo recibe el código en
//! un puerto local y lo cambia por tokens. El refresh token queda en el llavero del sistema.

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
/// Tiempo para que el usuario termine de aceptar en el navegador.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Permisos que pide Brote. Leer y enviar correos, manejar eventos y buscar contactos.
pub const SCOPES: &[&str] = &[
    "openid",
    "email",
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.send",
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/contacts.readonly",
    "https://www.googleapis.com/auth/contacts.other.readonly",
];

const KEYRING_SERVICE: &str = "Brote";
const KEY_REFRESH: &str = "google-refresh-token";
const KEY_SECRET: &str = "google-client-secret";

/// Credenciales de la app de Google (del proyecto de Google Cloud del usuario).
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    error_description: Option<String>,
}

#[derive(Serialize)]
pub struct ConnectResult {
    pub email: String,
}

fn entry(user: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, user).context("no pude abrir el llavero del sistema")
}

fn keyring_get(user: &str) -> Option<String> {
    entry(user).ok()?.get_password().ok().filter(|v| !v.is_empty())
}

fn keyring_set(user: &str, value: &str) -> Result<()> {
    entry(user)?.set_password(value).context("no pude guardar en el llavero")
}

fn keyring_clear(user: &str) {
    if let Ok(e) = entry(user) {
        let _ = e.delete_credential();
    }
}

/// Estado en memoria: copia del refresh token y el access token vigente.
#[derive(Default)]
pub struct GoogleAuth {
    refresh: Mutex<Option<String>>,
    secret: Mutex<Option<String>>,
    access: Mutex<Option<(String, Instant)>>,
}

impl GoogleAuth {
    /// Carga lo guardado. Se llama en el hilo principal al arrancar (ver settings.rs sobre Linux).
    pub fn load(&self) {
        *self.refresh.lock().unwrap() = keyring_get(KEY_REFRESH);
        *self.secret.lock().unwrap() = keyring_get(KEY_SECRET);
    }

    pub fn is_connected(&self) -> bool {
        self.refresh.lock().unwrap().is_some()
    }

    pub fn client_secret(&self) -> Option<String> {
        self.secret.lock().unwrap().clone()
    }

    pub fn set_client_secret(&self, secret: &str) -> Result<()> {
        keyring_set(KEY_SECRET, secret)?;
        *self.secret.lock().unwrap() = Some(secret.to_string());
        Ok(())
    }

    /// Abre el navegador en la pantalla de Google y espera a que el usuario acepte.
    pub async fn connect(&self, client: &ClientConfig, open_browser: impl Fn(&str) -> Result<()>) -> Result<ConnectResult> {
        let listener = TcpListener::bind("127.0.0.1:0").context("no pude abrir un puerto local para recibir la respuesta de Google")?;
        let redirect_uri = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let verifier = random_string(64);
        let state = random_string(24);
        let url = auth_url(&client.client_id, &redirect_uri, &pkce_challenge(&verifier), &state);
        log::info!("Google: abriendo el navegador para conectar");
        open_browser(&url)?;

        let expected_state = state.clone();
        let code = tokio::task::spawn_blocking(move || wait_for_code(listener, &expected_state, LOGIN_TIMEOUT))
            .await
            .map_err(|_| anyhow!("se interrumpió la espera del navegador"))??;

        let http = reqwest::Client::new();
        let response = http
            .post(TOKEN_URL)
            .form(&[
                ("code", code.as_str()),
                ("client_id", client.client_id.as_str()),
                ("client_secret", client.client_secret.as_str()),
                ("redirect_uri", redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
                ("code_verifier", verifier.as_str()),
            ])
            .send()
            .await
            .context("no pude conectarme con Google")?;
        let tokens = parse_token_response(response).await.inspect_err(|e| log::error!("Google: no se pudo canjear el código: {e}"))?;
        let refresh = tokens.refresh_token.clone().ok_or_else(|| anyhow!("Google no entregó un token de acceso permanente"))?;

        let email = fetch_email(&http, &tokens.access_token).await.unwrap_or_default();
        keyring_set(KEY_REFRESH, &refresh)?;
        *self.refresh.lock().unwrap() = Some(refresh);
        *self.access.lock().unwrap() = Some((tokens.access_token, Instant::now() + Duration::from_secs(tokens.expires_in)));
        log::info!("Google: conectado");
        Ok(ConnectResult { email })
    }

    /// Un access token vigente, renovándolo si hace falta.
    pub async fn access_token(&self, client: &ClientConfig) -> Result<String> {
        if let Some((token, expires)) = self.access.lock().unwrap().clone() {
            if expires > Instant::now() + Duration::from_secs(60) {
                return Ok(token);
            }
        }
        let refresh = self.refresh.lock().unwrap().clone().ok_or_else(|| anyhow!("Google no está conectado"))?;
        let response = reqwest::Client::new()
            .post(TOKEN_URL)
            .form(&[
                ("refresh_token", refresh.as_str()),
                ("client_id", client.client_id.as_str()),
                ("client_secret", client.client_secret.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .context("no pude conectarme con Google")?;
        let tokens = match parse_token_response(response).await {
            Ok(t) => t,
            Err(e) if format!("{e}").contains("invalid_grant") => {
                // El usuario revocó el acceso o el token venció: hay que volver a conectar.
                log::warn!("Google: el permiso venció o fue revocado (invalid_grant)");
                self.forget();
                bail!("Google no está conectado: el permiso venció, hay que volver a conectarlo en Ajustes");
            }
            Err(e) => {
                log::error!("Google: no se pudo renovar el acceso: {e}");
                return Err(e);
            }
        };
        log::info!("Google: acceso renovado");
        *self.access.lock().unwrap() = Some((tokens.access_token.clone(), Instant::now() + Duration::from_secs(tokens.expires_in)));
        Ok(tokens.access_token)
    }

    /// Revoca el permiso en Google y borra los tokens.
    pub async fn disconnect(&self) {
        let refresh = self.refresh.lock().unwrap().clone();
        if let Some(token) = refresh {
            let _ = reqwest::Client::new().post(REVOKE_URL).form(&[("token", token.as_str())]).send().await;
        }
        self.forget();
        log::info!("Google: desconectado");
    }

    fn forget(&self) {
        keyring_clear(KEY_REFRESH);
        *self.refresh.lock().unwrap() = None;
        *self.access.lock().unwrap() = None;
    }
}

async fn parse_token_response(response: reqwest::Response) -> Result<TokenResponse> {
    let status = response.status();
    let text = response.text().await?;
    if status.is_success() {
        return serde_json::from_str(&text).context("respuesta de Google inesperada");
    }
    match serde_json::from_str::<TokenError>(&text) {
        Ok(e) => bail!("{}: {}", e.error, e.error_description.unwrap_or_default()),
        Err(_) => bail!("Google respondió {status}"),
    }
}

async fn fetch_email(http: &reqwest::Client, token: &str) -> Result<String> {
    let v: serde_json::Value = http.get(USERINFO_URL).bearer_auth(token).send().await?.json().await?;
    Ok(v["email"].as_str().unwrap_or_default().to_string())
}

fn random_string(len: usize) -> String {
    rand::thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn auth_url(client_id: &str, redirect_uri: &str, challenge: &str, state: &str) -> String {
    let mut url = url::Url::parse(AUTH_URL).expect("URL fija válida");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &SCOPES.join(" "))
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        // offline + consent: así Google entrega un refresh token.
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    url.into()
}

/// Espera la redirección de Google en el puerto local y devuelve el código.
fn wait_for_code(listener: TcpListener, expected_state: &str, timeout: Duration) -> Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() > deadline {
            bail!("se acabó el tiempo para aceptar en el navegador");
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(result) = handle_redirect(stream, expected_state)? {
                    return result;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.into()),
        }
    }
}

/// Procesa una petición al puerto local. `None` si no era la redirección (por ejemplo, el favicon).
fn handle_redirect(mut stream: TcpStream, expected_state: &str) -> Result<Option<Result<String>>> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).unwrap_or(0);
    let request = String::from_utf8_lossy(&buf[..n]);
    let outcome = parse_redirect(&request, expected_state);
    let (status, message) = match &outcome {
        None => ("404 Not Found", "".to_string()),
        Some(Ok(_)) => ("200 OK", "Listo. Ya puedes cerrar esta pestaña y volver con Brote.".into()),
        Some(Err(e)) => ("400 Bad Request", format!("No se pudo conectar: {e}")),
    };
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Brote</title>\
         <body style=\"font:16px system-ui;display:grid;place-items:center;height:90vh\"><p>{message}</p></body>"
    );
    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    Ok(outcome)
}

/// Lee `GET /?code=...&state=...`. Comprueba `state` para descartar peticiones que no vienen de nuestro login.
pub fn parse_redirect(request: &str, expected_state: &str) -> Option<Result<String>> {
    let target = request.lines().next()?.split_whitespace().nth(1)?;
    let url = url::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != "/" {
        return None;
    }
    let get = |k: &str| url.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.into_owned());
    if let Some(error) = get("error") {
        return Some(Err(anyhow!(if error == "access_denied" { "no aceptaste los permisos".to_string() } else { error })));
    }
    let code = get("code")?;
    if get("state").as_deref() != Some(expected_state) {
        return Some(Err(anyhow!("la respuesta no corresponde a este inicio de sesión")));
    }
    Some(Ok(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_coincide_con_el_ejemplo_del_rfc_7636() {
        assert_eq!(pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn la_url_pide_refresh_token_y_los_permisos() {
        let url = auth_url("id.apps.googleusercontent.com", "http://127.0.0.1:5000", "abc", "st");
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("gmail.send"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5000"));
    }

    #[test]
    fn lee_la_redireccion_y_valida_el_state() {
        let ok = parse_redirect("GET /?state=st&code=4%2F0Ab HTTP/1.1\r\nHost: x\r\n", "st");
        assert_eq!(ok.unwrap().unwrap(), "4/0Ab");
        assert!(parse_redirect("GET /?state=otro&code=x HTTP/1.1", "st").unwrap().is_err());
        assert!(parse_redirect("GET /?error=access_denied HTTP/1.1", "st").unwrap().is_err());
        assert!(parse_redirect("GET /favicon.ico HTTP/1.1", "st").is_none());
    }

    #[test]
    fn espera_el_codigo_en_el_puerto_local() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            let mut favicon = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(favicon, "GET /favicon.ico HTTP/1.1\r\n\r\n").unwrap();
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET /?code=abc&state=st HTTP/1.1\r\n\r\n").unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out);
            assert!(out.starts_with("HTTP/1.1 200"));
        });
        assert_eq!(wait_for_code(listener, "st", Duration::from_secs(5)).unwrap(), "abc");
    }
}

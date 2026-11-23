//! Cliente HTTP autenticado para las APIs de Google.

use super::auth::{ClientConfig, GoogleAuth};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct Api {
    auth: Arc<GoogleAuth>,
    /// `None` si el usuario todavía no configura las credenciales de Google Cloud.
    client: Option<ClientConfig>,
    http: reqwest::Client,
}

impl Api {
    pub fn new(auth: Arc<GoogleAuth>, client: Option<ClientConfig>) -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().unwrap_or_default();
        Self { auth, client, http }
    }

    async fn token(&self) -> Result<String> {
        let client = self.client.as_ref().ok_or_else(|| anyhow!("Google no está conectado"))?;
        if !self.auth.is_connected() {
            bail!("Google no está conectado");
        }
        self.auth.access_token(client).await
    }

    async fn send(&self, build: impl Fn(&reqwest::Client) -> reqwest::RequestBuilder) -> Result<Value> {
        let token = self.token().await?;
        let response = build(&self.http).bearer_auth(token).send().await.context("no pude conectarme con Google")?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if status.is_success() {
            if text.trim().is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_str(&text).context("respuesta de Google inesperada");
        }
        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v["error"]["message"].as_str().map(String::from))
            .unwrap_or_default();
        match status.as_u16() {
            401 => bail!("Google no está conectado: rechazó el permiso, hay que volver a conectarlo en Ajustes"),
            403 => bail!("Google no dio permiso para esto ({detail}). Puede que falte activar la API en Google Cloud"),
            404 => bail!("no lo encontré en Google"),
            _ => bail!("Google respondió {status}: {detail}"),
        }
    }

    pub async fn get(&self, url: &str, query: &[(&str, String)]) -> Result<Value> {
        self.send(|h| h.get(url).query(query)).await
    }

    pub async fn post(&self, url: &str, query: &[(&str, String)], body: &Value) -> Result<Value> {
        self.send(|h| h.post(url).query(query).json(body)).await
    }

    pub async fn patch(&self, url: &str, query: &[(&str, String)], body: &Value) -> Result<Value> {
        self.send(|h| h.patch(url).query(query).json(body)).await
    }

    pub async fn delete(&self, url: &str, query: &[(&str, String)]) -> Result<Value> {
        self.send(|h| h.delete(url).query(query)).await
    }
}

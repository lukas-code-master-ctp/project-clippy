//! Cliente mínimo de OpenRouter (API compatible con OpenAI, con herramientas).

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

const URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "function_type")]
    pub kind: String,
    pub function: FunctionCall,
}

fn function_type() -> String {
    "function".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    /// JSON en texto, tal como lo manda el modelo.
    #[serde(default)]
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self { role: "system".into(), content: Some(text.into()), tool_calls: vec![], tool_call_id: None }
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: "user".into(), content: Some(text.into()), tool_calls: vec![], tool_call_id: None }
    }
    pub fn tool(call_id: &str, text: impl Into<String>) -> Self {
        Self { role: "tool".into(), content: Some(text.into()), tool_calls: vec![], tool_call_id: Some(call_id.into()) }
    }
}

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    choices: Vec<Choice>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(TIMEOUT).build()?)
}

/// Una vuelta de conversación. `tools` va en formato OpenAI.
pub async fn chat(key: &str, model: &str, messages: &[Message], tools: &[Value]) -> Result<Message> {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": 600,
        // Solo proveedores que no guardan ni entrenan con los datos (ver spec, Seguridad).
        "provider": { "data_collection": "deny" },
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }
    let started = std::time::Instant::now();
    let response = client()?
        .post(std::env::var("BROTE_OPENROUTER_URL").unwrap_or_else(|_| URL.into()))
        .bearer_auth(key)
        .header("X-Title", "Brote")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.without_url())
        .inspect_err(|e| log::error!("OpenRouter sin conexión: {e}"))
        .context("no pude conectarme con OpenRouter")?;
    let status = response.status();
    let text = response.text().await.context("OpenRouter cortó la respuesta")?;
    log::info!("OpenRouter {model}: {status} en {:.2} s", started.elapsed().as_secs_f32());
    let parsed: Response = serde_json::from_str(&text).map_err(|_| anyhow!("OpenRouter respondió {status}: {}", truncate(&text)))?;
    if let Some(err) = parsed.error {
        return Err(anyhow!("OpenRouter respondió {status}: {}", err.message));
    }
    if !status.is_success() {
        return Err(anyhow!("OpenRouter respondió {status}"));
    }
    parsed.choices.into_iter().next().map(|c| c.message).ok_or_else(|| anyhow!("OpenRouter no devolvió respuesta"))
}

/// Pregunta mínima para validar key y modelo.
pub async fn ping(key: &str, model: &str) -> Result<String> {
    let reply = chat(key, model, &[Message::user("Responde solo: listo")], &[]).await?;
    Ok(reply.content.unwrap_or_default().trim().to_string())
}

fn truncate(s: &str) -> String {
    s.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_una_respuesta_con_llamada_a_herramienta() {
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":null,
            "tool_calls":[{"id":"c1","type":"function","function":{"name":"fecha_y_hora","arguments":"{}"}}]}}]}"#;
        let r: Response = serde_json::from_str(raw).unwrap();
        let m = &r.choices[0].message;
        assert_eq!(m.tool_calls[0].function.name, "fecha_y_hora");
        assert_eq!(m.content, None);
    }

    #[test]
    fn serializa_el_resultado_de_una_herramienta() {
        let v = serde_json::to_value(Message::tool("c1", "ok")).unwrap();
        assert_eq!(v, json!({"role":"tool","content":"ok","tool_call_id":"c1"}));
    }
}

//! Herramientas que el modelo puede pedir. Cada una declara si es de lectura o sensible.

use super::policy::{Approval, Sensitivity};
use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T>> + Send>>;
pub type ReadFn = Box<dyn Fn(Value) -> BoxFuture<String> + Send + Sync>;
/// Arma la tarjeta: exactamente lo que se va a hacer. También valida los argumentos.
pub type CardFn = Box<dyn Fn(Value) -> BoxFuture<Card> + Send + Sync>;
/// Una herramienta sensible solo corre con una `Approval`, que consume.
pub type SensitiveFn = Box<dyn Fn(Value, Approval) -> BoxFuture<String> + Send + Sync>;

/// Contenido de la tarjeta de confirmación.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Card {
    pub title: String,
    pub fields: Vec<CardField>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardField {
    /// Nombre del argumento de la herramienta que este campo muestra.
    pub key: String,
    pub label: String,
    pub value: String,
    /// El usuario puede corregirlo antes de confirmar.
    pub editable: bool,
    pub multiline: bool,
}

impl CardField {
    pub fn fixed(key: &str, label: &str, value: impl Into<String>) -> Self {
        Self { key: key.into(), label: label.into(), value: value.into(), editable: false, multiline: false }
    }
    pub fn editable(key: &str, label: &str, value: impl Into<String>) -> Self {
        Self { editable: true, ..Self::fixed(key, label, value) }
    }
    pub fn editable_long(key: &str, label: &str, value: impl Into<String>) -> Self {
        Self { multiline: true, ..Self::editable(key, label, value) }
    }
}

pub enum Runner {
    Read(ReadFn),
    Sensitive { card: CardFn, run: SensitiveFn },
}

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
    pub runner: Runner,
}

impl Tool {
    #[allow(dead_code)]
    pub fn sensitivity(&self) -> Sensitivity {
        match self.runner {
            Runner::Read(_) => Sensitivity::Read,
            Runner::Sensitive { .. } => Sensitivity::Sensitive,
        }
    }

    /// Definición en formato OpenAI para mandar al modelo.
    pub fn spec(&self) -> Value {
        json!({
            "type": "function",
            "function": { "name": self.name, "description": self.description, "parameters": self.parameters }
        })
    }
}

#[derive(Default)]
pub struct Registry {
    tools: Vec<Tool>,
}

impl Registry {
    pub fn with(mut self, tool: Tool) -> Self {
        self.tools.push(tool);
        self
    }

    pub fn with_all(mut self, tools: Vec<Tool>) -> Self {
        self.tools.extend(tools);
        self
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.name == name)
    }

    pub fn specs(&self) -> Vec<Value> {
        self.tools.iter().map(Tool::spec).collect()
    }
}

/// Lee un argumento de texto obligatorio.
pub fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("falta {key}"))
}

pub fn arg_opt<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// Lista de textos: acepta un arreglo o un texto separado por comas.
pub fn arg_list(args: &Value, key: &str) -> Vec<String> {
    match args.get(key) {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        Some(Value::String(s)) => s.split([',', ';']).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
        _ => vec![],
    }
}

const DIAS: [&str; 7] = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"];

pub fn weekday_es(day: chrono::Weekday) -> &'static str {
    DIAS[day.num_days_from_monday() as usize]
}

pub fn now_text() -> String {
    use chrono::{Datelike, Local};
    let now = Local::now();
    format!("{} {} (zona horaria {})", weekday_es(now.weekday()), now.format("%d-%m-%Y, %H:%M"), now.format("%:z"))
}

pub fn fecha_y_hora() -> Tool {
    Tool {
        name: "fecha_y_hora",
        description: "Devuelve la fecha y la hora actuales del computador del usuario.",
        parameters: json!({ "type": "object", "properties": {} }),
        runner: Runner::Read(Box::new(|_| Box::pin(async { Ok(now_text()) }))),
    }
}

/// Valida que sea un enlace web normal. Nada de file:, javascript: ni programas locales.
pub fn parse_web_url(args: &Value) -> Result<String> {
    let url = arg_str(args, "url")?;
    let lower = url.to_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) || url.contains(char::is_whitespace) {
        return Err(anyhow!("solo puedo abrir direcciones web que empiecen con http:// o https://"));
    }
    Ok(url.to_string())
}

/// Abre un sitio en el navegador. Es sensible porque actúa fuera de Brote.
pub fn abrir_sitio_web(open: impl Fn(&str) -> Result<()> + Send + Sync + 'static) -> Tool {
    let open = std::sync::Arc::new(open);
    Tool {
        name: "abrir_sitio_web",
        description: "Abre una página web en el navegador predeterminado del usuario.",
        parameters: json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "Dirección completa, con https://" } },
            "required": ["url"]
        }),
        runner: Runner::Sensitive {
            card: Box::new(|args| {
                Box::pin(async move {
                    let url = parse_web_url(&args)?;
                    Ok(Card { title: "Abrir en tu navegador".into(), fields: vec![CardField::fixed("url", "Dirección", url)] })
                })
            }),
            run: Box::new(move |args, _approval: Approval| {
                let open = open.clone();
                Box::pin(async move {
                    let url = parse_web_url(&args)?;
                    open(&url)?;
                    Ok(format!("Abrí {url}"))
                })
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_acepta_enlaces_web() {
        assert!(parse_web_url(&json!({"url": "https://www.sii.cl"})).is_ok());
        assert!(parse_web_url(&json!({"url": "file:///C:/Windows/system32/cmd.exe"})).is_err());
        assert!(parse_web_url(&json!({"url": "javascript:alert(1)"})).is_err());
        assert!(parse_web_url(&json!({})).is_err());
    }

    #[test]
    fn la_sensibilidad_sale_del_tipo_de_herramienta() {
        assert_eq!(fecha_y_hora().sensitivity(), Sensitivity::Read);
        assert_eq!(abrir_sitio_web(|_| Ok(())).sensitivity(), Sensitivity::Sensitive);
    }

    #[test]
    fn las_listas_aceptan_arreglo_o_comas() {
        assert_eq!(arg_list(&json!({"a": ["x@y.cl", " z@w.cl "]}), "a"), vec!["x@y.cl", "z@w.cl"]);
        assert_eq!(arg_list(&json!({"a": "x@y.cl, z@w.cl;"}), "a"), vec!["x@y.cl", "z@w.cl"]);
        assert!(arg_list(&json!({}), "a").is_empty());
    }
}

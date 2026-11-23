//! Herramientas que el modelo puede pedir. Cada una declara si es de lectura o sensible.

use super::policy::{Approval, Sensitivity};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub type ReadFn = Box<dyn Fn(&Value) -> Result<String> + Send + Sync>;
/// Una herramienta sensible solo corre con una `Approval`, que consume.
pub type SensitiveFn = Box<dyn Fn(&Value, Approval) -> Result<String> + Send + Sync>;

pub enum Runner {
    Read(ReadFn),
    Sensitive {
        /// Texto para la tarjeta: exactamente lo que se va a hacer.
        preview: fn(&Value) -> Result<String>,
        run: SensitiveFn,
    },
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

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.name == name)
    }

    pub fn specs(&self) -> Vec<Value> {
        self.tools.iter().map(Tool::spec).collect()
    }
}

const DIAS: [&str; 7] = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"];

pub fn now_text() -> String {
    use chrono::{Datelike, Local};
    let now = Local::now();
    let dia = DIAS[now.weekday().num_days_from_monday() as usize];
    format!("{dia} {}", now.format("%d-%m-%Y, %H:%M"))
}

pub fn fecha_y_hora() -> Tool {
    Tool {
        name: "fecha_y_hora",
        description: "Devuelve la fecha y la hora actuales del computador del usuario.",
        parameters: json!({ "type": "object", "properties": {} }),
        runner: Runner::Read(Box::new(|_| Ok(now_text()))),
    }
}

/// Valida que sea un enlace web normal. Nada de file:, javascript: ni programas locales.
pub fn parse_web_url(args: &Value) -> Result<String> {
    let url = args.get("url").and_then(Value::as_str).ok_or_else(|| anyhow!("falta la URL"))?.trim();
    let lower = url.to_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) || url.contains(char::is_whitespace) {
        return Err(anyhow!("solo puedo abrir direcciones web que empiecen con http:// o https://"));
    }
    Ok(url.to_string())
}

/// Abre un sitio en el navegador. Es sensible porque actúa fuera de Brote.
pub fn abrir_sitio_web(open: impl Fn(&str) -> Result<()> + Send + Sync + 'static) -> Tool {
    Tool {
        name: "abrir_sitio_web",
        description: "Abre una página web en el navegador predeterminado del usuario.",
        parameters: json!({
            "type": "object",
            "properties": { "url": { "type": "string", "description": "Dirección completa, con https://" } },
            "required": ["url"]
        }),
        runner: Runner::Sensitive {
            preview: |args| Ok(format!("Abrir en tu navegador: {}", parse_web_url(args)?)),
            run: Box::new(move |args, _approval: Approval| {
                let url = parse_web_url(args)?;
                open(&url)?;
                Ok(format!("Abrí {url}"))
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
}

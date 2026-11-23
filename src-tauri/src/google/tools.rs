//! Herramientas de Google para el agente. Leer es libre; enviar y cambiar el calendario pasan por la tarjeta.

use super::api::Api;
use super::{calendar, contacts, gmail};
use crate::agent::tools::{arg_list, arg_opt, arg_str, Card, CardField, Runner, Tool};
use anyhow::{anyhow, bail, Result};
use chrono::Duration;
use serde_json::{json, Value};

/// Arma una herramienta de lectura a partir de una función async sobre la API.
macro_rules! read_tool {
    ($api:expr, $name:expr, $desc:expr, $params:expr, |$a:ident, $args:ident| $body:expr) => {{
        let api = $api.clone();
        Tool {
            name: $name,
            description: $desc,
            parameters: $params,
            runner: Runner::Read(Box::new(move |$args: Value| {
                let $a = api.clone();
                Box::pin(async move { $body })
            })),
        }
    }};
}

fn recipients(args: &Value) -> Result<Vec<String>> {
    let to = arg_list(args, "para");
    if to.is_empty() {
        bail!("falta el destinatario");
    }
    if let Some(bad) = to.iter().find(|t| !gmail::is_email(t)) {
        bail!("\"{bad}\" no es una dirección de correo válida; búscala con buscar_contacto");
    }
    Ok(to)
}

fn guests(args: &Value) -> Result<Vec<String>> {
    let list = arg_list(args, "invitados");
    if let Some(bad) = list.iter().find(|t| !gmail::is_email(t)) {
        bail!("\"{bad}\" no es una dirección de correo válida; búscala con buscar_contacto");
    }
    Ok(list)
}

/// Inicio y fin. Sin fin, dura una hora.
fn times(args: &Value) -> Result<(chrono::DateTime<chrono::FixedOffset>, chrono::DateTime<chrono::FixedOffset>)> {
    let start = calendar::parse_time(arg_str(args, "inicio")?)?;
    let end = match arg_opt(args, "fin") {
        Some(f) => calendar::parse_time(f)?,
        None => start + Duration::hours(1),
    };
    if end <= start {
        bail!("el evento termina antes de empezar");
    }
    Ok((start, end))
}

pub fn all(api: &Api) -> Vec<Tool> {
    vec![
        read_tool!(
            api,
            "buscar_contacto",
            "Busca el correo de una persona por su nombre en Google Contacts y en los remitentes de Gmail. Úsala siempre antes de escribirle o invitar a alguien.",
            json!({"type": "object", "properties": {"nombre": {"type": "string"}}, "required": ["nombre"]}),
            |a, args| {
                let name = arg_str(&args, "nombre")?.to_string();
                Ok(contacts::format(&contacts::search(&a, &name).await?, &name))
            }
        ),
        read_tool!(
            api,
            "buscar_correos",
            "Busca correos en Gmail con la sintaxis de búsqueda de Gmail (from:, to:, subject:, newer_than:7d, is:unread...). Devuelve id, remitente, asunto, fecha y un extracto.",
            json!({"type": "object", "properties": {
                "consulta": {"type": "string"},
                "cantidad": {"type": "integer", "description": "Máximo 10. Por defecto 5."}
            }, "required": ["consulta"]}),
            |a, args| {
                let q = arg_str(&args, "consulta")?.to_string();
                let n = args.get("cantidad").and_then(Value::as_u64).unwrap_or(5) as u32;
                Ok(gmail::format_summaries(&gmail::search(&a, &q, n).await?))
            }
        ),
        read_tool!(
            api,
            "leer_correo",
            "Lee el contenido completo de un correo por su id (obtenido con buscar_correos).",
            json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
            |a, args| gmail::read(&a, arg_str(&args, "id")?).await
        ),
        read_tool!(
            api,
            "ver_eventos",
            "Lista los eventos del calendario principal entre dos fechas (formato 2026-11-24T00:00, hora local).",
            json!({"type": "object", "properties": {
                "desde": {"type": "string"}, "hasta": {"type": "string"}
            }, "required": ["desde", "hasta"]}),
            |a, args| calendar::list(&a, arg_str(&args, "desde")?, arg_str(&args, "hasta")?).await
        ),
        enviar_correo(api),
        crear_evento(api),
        mover_evento(api),
        cancelar_evento(api),
    ]
}

fn enviar_correo(api: &Api) -> Tool {
    let card_api = api.clone();
    let run_api = api.clone();
    Tool {
        name: "enviar_correo",
        description: "Envía un correo desde la cuenta de Gmail del usuario. Para responder un correo existente, pasa su id en responder_a_id. El usuario lo revisa y puede editarlo antes de enviarlo.",
        parameters: json!({"type": "object", "properties": {
            "para": {"type": "array", "items": {"type": "string"}, "description": "Direcciones obtenidas con buscar_contacto"},
            "asunto": {"type": "string"},
            "cuerpo": {"type": "string", "description": "Texto del correo, en el tono del usuario, firmado con su nombre si lo sabes"},
            "responder_a_id": {"type": "string"}
        }, "required": ["para", "asunto", "cuerpo"]}),
        runner: Runner::Sensitive {
            card: Box::new(move |args| {
                let api = card_api.clone();
                Box::pin(async move {
                    let to = recipients(&args)?;
                    let subject = arg_str(&args, "asunto")?.to_string();
                    let body = arg_str(&args, "cuerpo")?.to_string();
                    let title = match arg_opt(&args, "responder_a_id") {
                        Some(id) => {
                            let (_, original) = gmail::reply_info(&api, id).await?;
                            format!("Responder «{original}»")
                        }
                        None => "Enviar correo".into(),
                    };
                    Ok(Card {
                        title,
                        fields: vec![
                            CardField::editable("para", "Para", to.join(", ")),
                            CardField::editable("asunto", "Asunto", subject),
                            CardField::editable_long("cuerpo", "Mensaje", body),
                        ],
                    })
                })
            }),
            run: Box::new(move |args, _approval| {
                let api = run_api.clone();
                Box::pin(async move {
                    let to = recipients(&args)?;
                    gmail::send(&api, &to, arg_str(&args, "asunto")?, arg_str(&args, "cuerpo")?, arg_opt(&args, "responder_a_id")).await
                })
            }),
        },
    }
}

fn crear_evento(api: &Api) -> Tool {
    let run_api = api.clone();
    Tool {
        name: "crear_evento",
        description: "Crea un evento en el calendario principal. Fechas en formato 2026-11-24T10:00 (hora local). Si hay invitados, Google les manda la invitación.",
        parameters: json!({"type": "object", "properties": {
            "titulo": {"type": "string"},
            "inicio": {"type": "string"},
            "fin": {"type": "string", "description": "Opcional. Por defecto, una hora después del inicio."},
            "invitados": {"type": "array", "items": {"type": "string"}, "description": "Correos obtenidos con buscar_contacto"},
            "lugar": {"type": "string"},
            "descripcion": {"type": "string"}
        }, "required": ["titulo", "inicio"]}),
        runner: Runner::Sensitive {
            card: Box::new(|args| {
                Box::pin(async move {
                    let (start, end) = times(&args)?;
                    let guests = guests(&args)?;
                    let mut fields = vec![
                        CardField::editable("titulo", "Título", arg_str(&args, "titulo")?),
                        CardField::fixed("cuando", "Cuándo", calendar::human_range(&start, &end)),
                    ];
                    if !guests.is_empty() {
                        fields.push(CardField::fixed("invitados", "Invitados (les llega la invitación)", guests.join(", ")));
                    }
                    if let Some(l) = arg_opt(&args, "lugar") {
                        fields.push(CardField::editable("lugar", "Lugar", l));
                    }
                    if let Some(d) = arg_opt(&args, "descripcion") {
                        fields.push(CardField::editable_long("descripcion", "Descripción", d));
                    }
                    Ok(Card { title: "Crear evento".into(), fields })
                })
            }),
            run: Box::new(move |args, _approval| {
                let api = run_api.clone();
                Box::pin(async move {
                    let (start, end) = times(&args)?;
                    let event = calendar::NewEvent {
                        title: arg_str(&args, "titulo")?.to_string(),
                        start,
                        end,
                        attendees: guests(&args)?,
                        location: arg_opt(&args, "lugar").map(String::from),
                        description: arg_opt(&args, "descripcion").map(String::from),
                    };
                    calendar::create(&api, &event).await
                })
            }),
        },
    }
}

fn event_when(e: &Value) -> String {
    let parse = |v: &Value| v["dateTime"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
    match (parse(&e["start"]), parse(&e["end"])) {
        (Some(s), Some(f)) => calendar::human_range(&s, &f),
        _ => e["start"]["date"].as_str().map(|d| format!("{d}, todo el día")).unwrap_or_default(),
    }
}

fn attendees_text(e: &Value) -> Option<String> {
    let list: Vec<String> = e["attendees"].as_array()?.iter().filter_map(|a| a["email"].as_str().map(String::from)).collect();
    (!list.is_empty()).then(|| list.join(", "))
}

fn mover_evento(api: &Api) -> Tool {
    let card_api = api.clone();
    let run_api = api.clone();
    Tool {
        name: "mover_evento",
        description: "Cambia la fecha u hora de un evento existente (id de ver_eventos). Sin fin, mantiene la duración original.",
        parameters: json!({"type": "object", "properties": {
            "id": {"type": "string"}, "inicio": {"type": "string"}, "fin": {"type": "string"}
        }, "required": ["id", "inicio"]}),
        runner: Runner::Sensitive {
            card: Box::new(move |args| {
                let api = card_api.clone();
                Box::pin(async move {
                    let event = calendar::get(&api, arg_str(&args, "id")?).await?;
                    let (start, end) = new_times(&args, &event)?;
                    let mut fields = vec![
                        CardField::fixed("evento", "Evento", event["summary"].as_str().unwrap_or("(sin título)")),
                        CardField::fixed("antes", "Antes", event_when(&event)),
                        CardField::fixed("ahora", "Ahora", calendar::human_range(&start, &end)),
                    ];
                    if let Some(a) = attendees_text(&event) {
                        fields.push(CardField::fixed("invitados", "Se les avisa a", a));
                    }
                    Ok(Card { title: "Mover evento".into(), fields })
                })
            }),
            run: Box::new(move |args, _approval| {
                let api = run_api.clone();
                Box::pin(async move {
                    let id = arg_str(&args, "id")?;
                    let event = calendar::get(&api, id).await?;
                    let (start, end) = new_times(&args, &event)?;
                    calendar::reschedule(&api, id, start, end).await
                })
            }),
        },
    }
}

/// Nuevo horario; sin fin, conserva la duración del evento.
fn new_times(args: &Value, event: &Value) -> Result<(chrono::DateTime<chrono::FixedOffset>, chrono::DateTime<chrono::FixedOffset>)> {
    let start = calendar::parse_time(arg_str(args, "inicio")?)?;
    let end = match arg_opt(args, "fin") {
        Some(f) => calendar::parse_time(f)?,
        None => {
            let parse = |v: &Value| v["dateTime"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
            let duration = match (parse(&event["start"]), parse(&event["end"])) {
                (Some(s), Some(e)) => e - s,
                _ => return Err(anyhow!("es un evento de todo el día; indica también la hora de término")),
            };
            start + duration
        }
    };
    if end <= start {
        bail!("el evento termina antes de empezar");
    }
    Ok((start, end))
}

fn cancelar_evento(api: &Api) -> Tool {
    let card_api = api.clone();
    let run_api = api.clone();
    Tool {
        name: "cancelar_evento",
        description: "Cancela (borra) un evento del calendario (id de ver_eventos). Si tiene invitados, Google les avisa.",
        parameters: json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
        runner: Runner::Sensitive {
            card: Box::new(move |args| {
                let api = card_api.clone();
                Box::pin(async move {
                    let event = calendar::get(&api, arg_str(&args, "id")?).await?;
                    let mut fields = vec![
                        CardField::fixed("evento", "Evento", event["summary"].as_str().unwrap_or("(sin título)")),
                        CardField::fixed("cuando", "Cuándo", event_when(&event)),
                    ];
                    if let Some(a) = attendees_text(&event) {
                        fields.push(CardField::fixed("invitados", "Se les avisa a", a));
                    }
                    Ok(Card { title: "Cancelar evento".into(), fields })
                })
            }),
            run: Box::new(move |args, _approval| {
                let api = run_api.clone();
                Box::pin(async move { calendar::cancel(&api, arg_str(&args, "id")?).await })
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_acepta_destinatarios_inventados() {
        assert!(recipients(&json!({"para": ["juan@x.cl"]})).is_ok());
        assert!(recipients(&json!({"para": ["Juan"]})).is_err());
        assert!(recipients(&json!({"para": []})).is_err());
    }

    #[test]
    fn un_evento_sin_fin_dura_una_hora() {
        let (s, e) = times(&json!({"inicio": "2026-11-24T10:00"})).unwrap();
        assert_eq!(e - s, Duration::hours(1));
        assert!(times(&json!({"inicio": "2026-11-24T10:00", "fin": "2026-11-24T09:00"})).is_err());
    }

    #[test]
    fn mover_conserva_la_duracion() {
        let event = json!({"start": {"dateTime": "2026-11-24T10:00:00-03:00"}, "end": {"dateTime": "2026-11-24T10:30:00-03:00"}});
        let (s, e) = new_times(&json!({"inicio": "2026-11-25T15:00:00-03:00"}), &event).unwrap();
        assert_eq!(e - s, Duration::minutes(30));
    }

    #[tokio::test]
    async fn la_tarjeta_del_correo_muestra_todo_y_es_editable() {
        let api = Api::new(std::sync::Arc::new(super::super::auth::GoogleAuth::default()), None);
        let tool = enviar_correo(&api);
        let Runner::Sensitive { card, .. } = &tool.runner else { panic!() };
        let c = card(json!({"para": ["juan@x.cl"], "asunto": "Pedido", "cuerpo": "Mañana llega."})).await.unwrap();
        assert_eq!(c.title, "Enviar correo");
        assert!(c.fields.iter().all(|f| f.editable));
        assert_eq!(c.fields[2].value, "Mañana llega.");
        assert!(card(json!({"para": ["Juan"], "asunto": "x", "cuerpo": "y"})).await.is_err());
    }
}

//! Google Calendar: ver, crear, mover y cancelar eventos del calendario principal.

use super::api::Api;
use crate::agent::tools::weekday_es;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone};
use serde_json::{json, Value};

const BASE: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";

/// Interpreta una fecha u hora. Sin zona horaria, se toma la del computador.
/// Acepta "2026-11-24T10:00", "2026-11-24 10:00", "2026-11-24" o RFC 3339 completo.
pub fn parse_time(s: &str) -> Result<DateTime<FixedOffset>> {
    let s = s.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt);
    }
    let naive = ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
        .or_else(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)))
        .ok_or_else(|| anyhow!("no entendí la fecha \"{s}\"; usa el formato 2026-11-24T10:00"))?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.fixed_offset())
        .ok_or_else(|| anyhow!("la hora {s} no existe en tu zona horaria"))
}

/// "martes 24-11-2026, 10:00"
pub fn human(dt: &DateTime<FixedOffset>) -> String {
    format!("{} {}", weekday_es(dt.weekday()), dt.format("%d-%m-%Y, %H:%M"))
}

/// "martes 24-11-2026, 10:00 a 11:00" (o con fecha en ambos extremos si cambian de día).
pub fn human_range(start: &DateTime<FixedOffset>, end: &DateTime<FixedOffset>) -> String {
    if start.date_naive() == end.date_naive() {
        format!("{} a {}", human(start), end.format("%H:%M"))
    } else {
        format!("{} a {}", human(start), human(end))
    }
}

fn event_time(v: &Value) -> String {
    if let Some(dt) = v["dateTime"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()) {
        return human(&dt);
    }
    v["date"].as_str().map(|d| format!("{d} (todo el día)")).unwrap_or_default()
}

pub fn describe_event(e: &Value) -> String {
    let attendees = e["attendees"].as_array().map(|a| a.len()).unwrap_or(0);
    let mut line = format!(
        "- id {} | {} | de {} a {}",
        e["id"].as_str().unwrap_or_default(),
        e["summary"].as_str().unwrap_or("(sin título)"),
        event_time(&e["start"]),
        event_time(&e["end"])
    );
    if let Some(loc) = e["location"].as_str() {
        line.push_str(&format!(" | lugar: {loc}"));
    }
    if attendees > 0 {
        line.push_str(&format!(" | {attendees} invitados"));
    }
    line
}

pub async fn list(api: &Api, from: &str, to: &str) -> Result<String> {
    let (from, to) = (parse_time(from)?, parse_time(to)?);
    let res = api
        .get(
            BASE,
            &[
                ("timeMin", from.to_rfc3339()),
                ("timeMax", to.to_rfc3339()),
                ("singleEvents", "true".into()),
                ("orderBy", "startTime".into()),
                ("maxResults", "25".into()),
            ],
        )
        .await?;
    let items = res["items"].as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return Ok(format!("No hay eventos entre {} y {}.", human(&from), human(&to)));
    }
    Ok(items.iter().map(describe_event).collect::<Vec<_>>().join("\n"))
}

pub async fn get(api: &Api, id: &str) -> Result<Value> {
    api.get(&format!("{BASE}/{}", urlencode(id)), &[]).await
}

fn urlencode(id: &str) -> String {
    url::form_urlencoded::byte_serialize(id.as_bytes()).collect()
}

/// Con invitados, Google les avisa del cambio por correo.
fn send_updates(has_attendees: bool) -> (&'static str, String) {
    ("sendUpdates", if has_attendees { "all" } else { "none" }.into())
}

pub struct NewEvent {
    pub title: String,
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
    pub attendees: Vec<String>,
    pub location: Option<String>,
    pub description: Option<String>,
}

pub fn event_body(e: &NewEvent) -> Value {
    let mut body = json!({
        "summary": e.title,
        "start": { "dateTime": e.start.to_rfc3339() },
        "end": { "dateTime": e.end.to_rfc3339() },
    });
    if !e.attendees.is_empty() {
        body["attendees"] = Value::Array(e.attendees.iter().map(|a| json!({ "email": a })).collect());
    }
    if let Some(l) = &e.location {
        body["location"] = json!(l);
    }
    if let Some(d) = &e.description {
        body["description"] = json!(d);
    }
    body
}

pub async fn create(api: &Api, e: &NewEvent) -> Result<String> {
    let created = api.post(BASE, &[send_updates(!e.attendees.is_empty())], &event_body(e)).await?;
    Ok(format!("Evento creado: {} ({})", e.title, human_range(&e.start, &e.end)).to_string() + &link(&created))
}

pub async fn reschedule(api: &Api, id: &str, start: DateTime<FixedOffset>, end: DateTime<FixedOffset>) -> Result<String> {
    let current = get(api, id).await?;
    let has_attendees = current["attendees"].as_array().is_some_and(|a| !a.is_empty());
    let body = json!({ "start": { "dateTime": start.to_rfc3339() }, "end": { "dateTime": end.to_rfc3339() } });
    api.patch(&format!("{BASE}/{}", urlencode(id)), &[send_updates(has_attendees)], &body).await?;
    Ok(format!("Evento movido a {}", human_range(&start, &end)))
}

pub async fn cancel(api: &Api, id: &str) -> Result<String> {
    let current = get(api, id).await?;
    let has_attendees = current["attendees"].as_array().is_some_and(|a| !a.is_empty());
    api.delete(&format!("{BASE}/{}", urlencode(id)), &[send_updates(has_attendees)]).await?;
    Ok(format!("Evento cancelado: {}", current["summary"].as_str().unwrap_or("(sin título)")))
}

fn link(event: &Value) -> String {
    event["htmlLink"].as_str().map(|l| format!(". Enlace: {l}")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_fechas_locales_y_completas() {
        let local = parse_time("2026-11-24T10:00").unwrap();
        assert_eq!(local.format("%Y-%m-%d %H:%M").to_string(), "2026-11-24 10:00");
        assert_eq!(parse_time("2026-11-24 10:00").unwrap(), local);
        assert_eq!(parse_time("2026-11-24").unwrap().format("%H:%M").to_string(), "00:00");
        let full = parse_time("2026-11-24T10:00:00-03:00").unwrap();
        assert_eq!(full.offset().local_minus_utc(), -3 * 3600);
        assert!(parse_time("mañana a las 10").is_err());
    }

    #[test]
    fn rangos_legibles_en_espanol() {
        let a = parse_time("2026-11-24T10:00:00-03:00").unwrap();
        let b = parse_time("2026-11-24T11:30:00-03:00").unwrap();
        assert_eq!(human_range(&a, &b), "martes 24-11-2026, 10:00 a 11:30");
    }

    #[test]
    fn el_evento_lleva_invitados_solo_si_hay() {
        let start = parse_time("2026-11-24T10:00:00-03:00").unwrap();
        let mut e = NewEvent { title: "Reunión".into(), start, end: start, attendees: vec![], location: None, description: None };
        assert!(event_body(&e).get("attendees").is_none());
        e.attendees.push("juan@x.cl".into());
        assert_eq!(event_body(&e)["attendees"][0]["email"], "juan@x.cl");
        assert_eq!(event_body(&e)["start"]["dateTime"], "2026-11-24T10:00:00-03:00");
    }
}

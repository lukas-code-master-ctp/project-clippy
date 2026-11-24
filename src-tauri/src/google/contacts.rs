//! Búsqueda de contactos: Google Contacts, "otros contactos" y remitentes de Gmail.

use super::api::Api;
use super::gmail;
use anyhow::Result;
use serde_json::Value;

const PEOPLE: &str = "https://people.googleapis.com/v1";

#[derive(Debug, Clone, PartialEq)]
pub struct Contact {
    pub name: String,
    pub email: String,
    pub source: &'static str,
}

fn from_people(results: &Value, person_key: Option<&str>, source: &'static str) -> Vec<Contact> {
    let mut out = vec![];
    for r in results["results"].as_array().cloned().unwrap_or_default() {
        let person = match person_key {
            Some(k) => r[k].clone(),
            None => r["person"].clone(),
        };
        let name = person["names"][0]["displayName"].as_str().unwrap_or_default().to_string();
        for e in person["emailAddresses"].as_array().cloned().unwrap_or_default() {
            if let Some(email) = e["value"].as_str() {
                out.push(Contact { name: name.clone(), email: email.to_lowercase(), source });
            }
        }
    }
    out
}

/// Separa "Juan Pérez <juan@x.cl>" en nombre y correo.
pub fn parse_from_header(from: &str) -> Option<(String, String)> {
    let from = from.trim();
    if let (Some(start), Some(end)) = (from.rfind('<'), from.rfind('>')) {
        let email = from[start + 1..end].trim().to_lowercase();
        let name = from[..start].trim().trim_matches('"').to_string();
        return gmail::is_email(&email).then_some((name, email));
    }
    gmail::is_email(from).then(|| (String::new(), from.to_lowercase()))
}

pub async fn search(api: &Api, name: &str) -> Result<Vec<Contact>> {
    let mut all = vec![];
    let read_mask = ("readMask", "names,emailAddresses".to_string());

    // Google pide una búsqueda vacía de "calentamiento" antes de la real.
    let _ = api.get(&format!("{PEOPLE}/people:searchContacts"), &[("query", String::new()), read_mask.clone()]).await;
    if let Ok(r) = api.get(&format!("{PEOPLE}/people:searchContacts"), &[("query", name.to_string()), read_mask.clone()]).await {
        all.extend(from_people(&r, None, "contactos"));
    }
    let other_mask = ("readMask", "names,emailAddresses".to_string());
    if let Ok(r) = api.get(&format!("{PEOPLE}/otherContacts:search"), &[("query", name.to_string()), other_mask]).await {
        all.extend(from_people(&r, None, "otros contactos"));
    }
    if let Ok(mails) = gmail::search(api, &format!("from:({name})"), 5).await {
        for m in mails {
            if let Some((n, e)) = parse_from_header(&m.from) {
                all.push(Contact { name: n, email: e, source: "remitentes de Gmail" });
            }
        }
    }
    Ok(dedupe(all))
}

fn dedupe(list: Vec<Contact>) -> Vec<Contact> {
    let mut out: Vec<Contact> = vec![];
    for c in list {
        match out.iter_mut().find(|o| o.email == c.email) {
            Some(existing) if existing.name.is_empty() => existing.name = c.name,
            Some(_) => {}
            None => out.push(c),
        }
    }
    out
}

pub fn format(list: &[Contact], query: &str) -> String {
    if list.is_empty() {
        return format!("No encontré a nadie que se llame \"{query}\" en contactos ni en Gmail.");
    }
    let lines: Vec<String> = list
        .iter()
        .map(|c| format!("- {} <{}> (de {})", if c.name.is_empty() { "(sin nombre)" } else { &c.name }, c.email, c.source))
        .collect();
    let note = if list.len() > 1 { "\nHay más de una opción: pregunta al usuario cuál antes de seguir." } else { "" };
    format!("{}{note}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn separa_nombre_y_correo_del_remitente() {
        assert_eq!(parse_from_header("\"Juan Pérez\" <Juan@X.cl>"), Some(("Juan Pérez".into(), "juan@x.cl".into())));
        assert_eq!(parse_from_header("juan@x.cl"), Some(("".into(), "juan@x.cl".into())));
        assert_eq!(parse_from_header("Juan"), None);
    }

    #[test]
    fn junta_resultados_sin_repetir_correos() {
        let people = json!({"results": [{"person": {"names": [{"displayName": "Juan Pérez"}], "emailAddresses": [{"value": "juan@x.cl"}]}}]});
        let mut list = from_people(&people, None, "contactos");
        list.push(Contact { name: "".into(), email: "juan@x.cl".into(), source: "remitentes de Gmail" });
        list.push(Contact { name: "Juan Soto".into(), email: "jsoto@y.cl".into(), source: "remitentes de Gmail" });
        let list = dedupe(list);
        assert_eq!(list.len(), 2);
        assert!(format(&list, "Juan").contains("pregunta al usuario cuál"));
    }
}

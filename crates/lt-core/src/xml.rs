//! Conversion XML -> JSON et outils SOAP.
//!
//! Les préfixes de namespace sont retirés (`<soap:Envelope>` devient `Envelope`),
//! les attributs sont exposés sous `@nom`, le texte sous `#text`, et les éléments
//! répétés deviennent des listes.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde_json::{Map, Value};
use rust_i18n::t;
use std::fmt;

#[derive(Debug)]
pub struct XmlError(String);

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for XmlError {}

#[derive(Default, Clone)]
struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<(String, Node)>,
    text: String,
}

fn local_name(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    match name.rsplit_once(':') {
        Some((_, local)) => local.to_string(),
        None => name.to_string(),
    }
}

fn read_start(element: &BytesStart<'_>) -> Result<Node, XmlError> {
    let name = local_name(element.name().as_ref());
    let mut attrs = Vec::new();
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| XmlError(error.to_string()))?;
        let key = String::from_utf8_lossy(attribute.key.as_ref()).to_string();
        if key == "xmlns" || key.starts_with("xmlns:") {
            continue;
        }
        let value = attribute
            .unescape_value()
            .map_err(|error| XmlError(error.to_string()))?
            .into_owned();
        attrs.push((local_name(key.as_bytes()), value));
    }
    Ok(Node {
        name,
        attrs,
        ..Node::default()
    })
}

fn attach(stack: &mut [Node], root: &mut Vec<(String, Node)>, node: Node) {
    match stack.last_mut() {
        Some(parent) => parent.children.push((node.name.clone(), node)),
        None => root.push((node.name.clone(), node)),
    }
}

pub fn parse_xml(xml: &str) -> Result<Value, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Vec<(String, Node)> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => stack.push(read_start(&element)?),
            Ok(Event::Empty(element)) => {
                let node = read_start(&element)?;
                attach(&mut stack, &mut root, node);
            }
            Ok(Event::End(_)) => {
                let Some(node) = stack.pop() else {
                    return Err(XmlError(t!("xml.error.unmatched_close").to_string()));
                };
                attach(&mut stack, &mut root, node);
            }
            Ok(Event::Text(text)) => {
                if let Some(top) = stack.last_mut() {
                    let decoded = text
                        .unescape()
                        .map_err(|error| XmlError(error.to_string()))?;
                    top.text.push_str(&decoded);
                }
            }
            Ok(Event::CData(cdata)) => {
                if let Some(top) = stack.last_mut() {
                    top.text
                        .push_str(&String::from_utf8_lossy(cdata.into_inner().as_ref()));
                }
            }
            Ok(Event::Decl(_))
            | Ok(Event::PI(_))
            | Ok(Event::Comment(_))
            | Ok(Event::DocType(_)) => {}
            Ok(Event::Eof) => break,
            Err(error) => return Err(XmlError(error.to_string())),
        }
    }

    if !stack.is_empty() {
        return Err(XmlError(t!("xml.error.incomplete").to_string()));
    }
    if root.is_empty() {
        return Err(XmlError(t!("xml.error.empty").to_string()));
    }
    Ok(children_to_value(root))
}

fn children_to_value(children: Vec<(String, Node)>) -> Value {
    let mut grouped: std::collections::BTreeMap<String, Vec<Value>> = Default::default();
    for (name, node) in children {
        grouped.entry(name).or_default().push(node_to_value(&node));
    }
    let mut map = Map::new();
    for (name, mut values) in grouped {
        let value = if values.len() == 1 {
            values.pop().unwrap_or(Value::Null)
        } else {
            Value::Array(values)
        };
        map.insert(name, value);
    }
    Value::Object(map)
}

fn node_to_value(node: &Node) -> Value {
    let text = node.text.trim();
    if node.children.is_empty() && node.attrs.is_empty() {
        return Value::String(text.to_string());
    }
    let mut map = Map::new();
    for (key, value) in &node.attrs {
        map.insert(format!("@{key}"), Value::String(value.clone()));
    }
    if let Value::Object(children) = children_to_value(node.children.clone()) {
        for (key, value) in children {
            map.insert(key, value);
        }
    }
    if !text.is_empty() {
        map.insert("#text".to_string(), Value::String(text.to_string()));
    }
    Value::Object(map)
}

pub const SOAP_NS_11: &str = "http://schemas.xmlsoap.org/soap/envelope/";
pub const SOAP_NS_12: &str = "http://www.w3.org/2003/05/soap-envelope";

/// En-têtes HTTP d'une requête SOAP.
pub fn soap_headers(version: &str, action: &str) -> Vec<(String, String)> {
    if version == "1.2" {
        let mut content_type = "application/soap+xml; charset=utf-8".to_string();
        if !action.is_empty() {
            content_type.push_str(&format!("; action=\"{action}\""));
        }
        vec![("Content-Type".to_string(), content_type)]
    } else {
        vec![
            ("Content-Type".to_string(), "text/xml; charset=utf-8".to_string()),
            ("SOAPAction".to_string(), format!("\"{action}\"")),
        ]
    }
}

pub fn soap_template(version: &str) -> String {
    let namespace = if version == "1.2" {
        SOAP_NS_12
    } else {
        SOAP_NS_11
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<soapenv:Envelope xmlns:soapenv="{namespace}" xmlns:ns="http://exemple.com/service">
  <soapenv:Header/>
  <soapenv:Body>
    <ns:MaMethode>
      <ns:parametre>{{{{variable}}}}</ns:parametre>
    </ns:MaMethode>
  </soapenv:Body>
</soapenv:Envelope>"#
    )
}

/// Message de la SOAP Fault, ou `None` si la réponse n'en contient pas.
pub fn soap_fault(document: &Value) -> Option<String> {
    let fault = document.get("Envelope")?.get("Body")?.get("Fault")?;
    match fault {
        Value::Object(map) => {
            let reason = map.get("faultstring").or_else(|| {
                map.get("Reason")
                    .and_then(|reason| reason.get("Text"))
                    .map(|text| match text {
                        Value::Object(map) => map.get("#text").unwrap_or(text),
                        other => other,
                    })
            });
            Some(match reason {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => "SOAP Fault".to_string(),
            })
        }
        other => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_soap_response() {
        let xml = r#"<?xml version="1.0"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <ns:GetBalanceResponse xmlns:ns="http://exemple.com/">
      <ns:balance currency="XOF">12500</ns:balance>
      <ns:item>1</ns:item>
      <ns:item>2</ns:item>
    </ns:GetBalanceResponse>
  </soap:Body>
</soap:Envelope>"#;
        let document = parse_xml(xml).unwrap();
        assert_eq!(
            document["Envelope"]["Body"]["GetBalanceResponse"]["balance"],
            json!({"@currency": "XOF", "#text": "12500"})
        );
        assert_eq!(
            document["Envelope"]["Body"]["GetBalanceResponse"]["item"],
            json!(["1", "2"])
        );
        assert!(soap_fault(&document).is_none());
    }

    #[test]
    fn detects_fault() {
        let xml = r#"<Envelope><Body><Fault><faultcode>500</faultcode><faultstring>Boom</faultstring></Fault></Body></Envelope>"#;
        let document = parse_xml(xml).unwrap();
        assert_eq!(soap_fault(&document).as_deref(), Some("Boom"));
    }

    #[test]
    fn rejects_empty_xml() {
        assert!(parse_xml("").is_err());
        assert!(parse_xml("   ").is_err());
    }
}

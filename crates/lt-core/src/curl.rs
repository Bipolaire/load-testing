//! Import d'une commande curl (Postman, navigateur, SoapUI…).

use crate::models::{Header, Step, StepType};
use base64::Engine;
use rust_i18n::t;
use url::Url;

#[derive(Debug, Clone)]
pub struct ParsedCurl {
    pub base_url: String,
    pub step: Step,
    pub notes: Vec<String>,
}

/// Découpage proche du shell : gère quotes, échappements et continuations
/// de ligne (`\`, `^` sous cmd.exe, backtick sous PowerShell).
pub fn tokenize(input: &str) -> Vec<String> {
    let input = input
        .replace("\\\r\n", " ")
        .replace("\\\n", " ")
        .replace("^\r\n", " ")
        .replace("^\n", " ")
        .replace("`\r\n", " ")
        .replace("`\n", " ");

    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    let mut chars = input.chars().peekable();

    while let Some(character) = chars.next() {
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            } else if character == '\\'
                && active_quote == '"'
                && matches!(chars.peek(), Some('"') | Some('\\') | Some('$') | Some('`'))
            {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            } else {
                current.push(character);
            }
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                has = true;
            }
            whitespace if whitespace.is_whitespace() => {
                if !current.is_empty() || has {
                    out.push(std::mem::take(&mut current));
                    has = false;
                }
            }
            '\\' | '^' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() || has {
        out.push(current);
    }
    out
}

fn split_header(value: &str) -> Option<Header> {
    let (key, value) = value.split_once(':')?;
    Some(Header {
        key: key.trim().to_string(),
        value: value.trim().to_string(),
        ..Header::default()
    })
}

fn encode_form_value(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub fn parse_curl(command: &str) -> Result<ParsedCurl, String> {
    let tokens = tokenize(command.trim());
    let Some(first) = tokens.first() else {
        return Err(t!("curl.error.empty").to_string());
    };
    let first_lower = first.to_lowercase();
    if !(first_lower == "curl" || first_lower == "curl.exe") {
        return Err(t!("curl.error.not_curl").to_string());
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<Header> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut form: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    let with_value = [
        "-X",
        "--request",
        "-H",
        "--header",
        "-d",
        "--data",
        "--data-raw",
        "--data-binary",
        "--data-ascii",
        "--data-urlencode",
        "-u",
        "--user",
        "--url",
        "-b",
        "--cookie",
        "-A",
        "--user-agent",
        "-e",
        "--referer",
        "-o",
        "--output",
        "-m",
        "--max-time",
        "--connect-timeout",
    ];

    let mut index = 1;
    while index < tokens.len() {
        let argument = tokens[index].clone();
        if !argument.starts_with('-') || argument == "-" {
            url.get_or_insert(argument);
            index += 1;
            continue;
        }

        let (flag, inline) = if argument.starts_with("--") {
            match argument.split_once('=') {
                Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
                None => (argument.clone(), None),
            }
        } else {
            (argument.clone(), None)
        };

        let mut value = inline;
        if value.is_none() && with_value.contains(&flag.as_str()) {
            index += 1;
            value = tokens.get(index).cloned();
        }

        match flag.as_str() {
            "-X" | "--request" => {
                if let Some(value) = value {
                    method = Some(value.to_uppercase());
                }
            }
            "-H" | "--header" => {
                if let Some(value) = value
                    && let Some(header) = split_header(&value) {
                        headers.push(header);
                    }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" => {
                if let Some(value) = value {
                    data.push(value);
                }
            }
            "--data-urlencode" => {
                if let Some(value) = value {
                    match value.split_once('=') {
                        Some((key, raw)) => {
                            form.push(format!("{key}={}", encode_form_value(raw)));
                        }
                        None => form.push(encode_form_value(&value)),
                    }
                }
            }
            "-u" | "--user" => {
                if let Some(value) = value {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(value);
                    headers.push(Header {
                        key: "Authorization".to_string(),
                        value: format!("Basic {encoded}"),
                        ..Header::default()
                    });
                }
            }
            "--url" => url = value,
            "-A" | "--user-agent" => {
                if let Some(value) = value {
                    headers.push(Header {
                        key: "User-Agent".to_string(),
                        value,
                        ..Header::default()
                    });
                }
            }
            "-k" | "--insecure" => {
                notes.push(t!("curl.note.insecure_ignored").to_string());
            }
            _ => {}
        }
        index += 1;
    }

    let url = url.ok_or_else(|| t!("curl.error.no_url").to_string())?;
    let mut body = data.join("&");
    if !form.is_empty() {
        if !body.is_empty() {
            body.push('&');
        }
        body.push_str(&form.join("&"));
        if !headers
            .iter()
            .any(|header| header.key.eq_ignore_ascii_case("content-type"))
        {
            headers.push(Header {
                key: "Content-Type".to_string(),
                value: "application/x-www-form-urlencoded".to_string(),
                ..Header::default()
            });
        }
    }
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
        body = serde_json::to_string_pretty(&parsed).unwrap_or(body);
    }

    let normalized = if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("https://{url}")
    };
    let parsed = Url::parse(&normalized).map_err(|error| t!("curl.error.invalid_url", error = error.to_string()).to_string())?;

    let mut kept = Vec::new();
    for header in headers {
        let key = header.key.to_lowercase();
        if key == "authorization" || key == "apikey" {
            notes.push(t!("curl.note.header_removed", name = header.key).to_string());
            continue;
        }
        if key == "cookie" {
            notes.push(t!("curl.note.cookie_removed").to_string());
            continue;
        }
        kept.push(header);
    }

    let path = if parsed.query().is_some() {
        format!("{}{}", parsed.path(), parsed.query().map(|q| format!("?{q}")).unwrap_or_default())
    } else {
        parsed.path().to_string()
    };

    let mut step = Step {
        method: method
            .unwrap_or_else(|| if body.is_empty() { "GET".into() } else { "POST".into() }),
        path,
        headers: kept,
        body,
        ..Step::default()
    };
    step.name = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty()).rfind(|segment| {
                    !(segment.to_lowercase().starts_with('v')
                        && segment[1..].chars().all(|c| c.is_ascii_digit()))
                        && !segment.chars().all(|c| c.is_ascii_digit())
                })
                .unwrap_or("requete")
                .to_string()
        })
        .unwrap_or_else(|| "requete".to_string());

    let header_value = |name: &str| {
        step.headers
            .iter()
            .find(|header| header.key.eq_ignore_ascii_case(name))
            .map(|header| header.value.clone())
    };

    let is_soap = step
        .body
        .to_lowercase()
        .contains("<envelope")
        || step.body.contains(":Envelope")
        || header_value("content-type")
            .map(|value| value.to_lowercase().contains("soap+xml"))
            .unwrap_or(false);
    if is_soap {
        let content_type = header_value("content-type").unwrap_or_default();
        step.step_type = StepType::Soap;
        step.soap_version = if content_type.to_lowercase().contains("soap+xml")
            || step.body.contains("2003/05/soap-envelope")
        {
            "1.2".to_string()
        } else {
            "1.1".to_string()
        };
        step.soap_action = header_value("soapaction")
            .or_else(|| {
                let lower = content_type.to_lowercase();
                lower
                    .split("action=")
                    .nth(1)
                    .map(|rest| rest.trim_matches(['"', ';']).to_string())
            })
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        step.headers.retain(|header| {
            !header.key.eq_ignore_ascii_case("content-type")
                && !header.key.eq_ignore_ascii_case("soapaction")
        });
        if let Some(operation) = operation_name(&step.body) {
            step.name = operation;
        }
    }

    let base_url = parsed.origin().ascii_serialization();
    Ok(ParsedCurl {
        base_url,
        step,
        notes,
    })
}

fn operation_name(body: &str) -> Option<String> {
    let lower = body.to_lowercase();
    let body_start = lower.find(":body")?;
    let after_body = &body[body_start..];
    let open = after_body.find('>')?;
    let rest = after_body[open + 1..].trim_start();
    let rest = rest.strip_prefix('<')?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == ':')
        .collect();
    Some(name.rsplit(':').next().unwrap_or(&name).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_quotes_and_continuations() {
        let tokens = tokenize("curl -X POST 'https://x/y' \\\n -H \"A: b\" -d '{\"a\": 1}'");
        assert_eq!(tokens[0], "curl");
        assert_eq!(tokens[1], "-X");
        assert_eq!(tokens[2], "POST");
        assert_eq!(tokens[3], "https://x/y");
        assert_eq!(tokens[4], "-H");
        assert_eq!(tokens[5], "A: b");
    }

    #[test]
    fn parses_json_post() {
        let parsed = parse_curl(
            "curl --location 'https://api.exemple.com/v1/orders?x=1' \
             --header 'Content-Type: application/json' \
             --header 'Authorization: Bearer secret' \
             --data '{\"id\": 1}'",
        )
        .unwrap();
        assert_eq!(parsed.base_url, "https://api.exemple.com");
        assert_eq!(parsed.step.method, "POST");
        assert_eq!(parsed.step.path, "/v1/orders?x=1");
        assert_eq!(parsed.step.name, "orders");
        assert!(parsed.step.headers.iter().any(|h| h.key == "Content-Type"));
        assert!(!parsed
            .step
            .headers
            .iter()
            .any(|h| h.key.eq_ignore_ascii_case("authorization")));
        assert!(parsed.notes.iter().any(|note| note.contains("Authorization")));
    }

    #[test]
    fn parses_soap_request() {
        let parsed = parse_curl(
            "curl -X POST https://api.exemple.com/services/MonService \
             -H 'Content-Type: text/xml; charset=utf-8' \
             -H 'SOAPAction: \"http://exemple.com/MaMethode\"' \
             -d '<soapenv:Envelope xmlns:soapenv=\"http://schemas.xmlsoap.org/soap/envelope/\"><soapenv:Body><ns:MaMethode xmlns:ns=\"x\"><ns:a>1</ns:a></ns:MaMethode></soapenv:Body></soapenv:Envelope>'",
        )
        .unwrap();
        assert_eq!(parsed.step.step_type, StepType::Soap);
        assert_eq!(parsed.step.soap_version, "1.1");
        assert_eq!(parsed.step.soap_action, "http://exemple.com/MaMethode");
        assert_eq!(parsed.step.name, "MaMethode");
        assert_eq!(parsed.step.method, "POST");
        assert_eq!(parsed.step.path, "/services/MonService");
    }

    #[test]
    fn rejects_non_curl() {
        assert!(parse_curl("wget https://x").is_err());
    }
}

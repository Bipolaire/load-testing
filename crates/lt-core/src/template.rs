//! Rendu des variables `{{nom}}` dans les URL, en-têtes et corps.

use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

fn variable_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{\s*([\w.-]+)\s*\}\}").expect("regex valide"))
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn random_value() -> u64 {
    // Générateur simple, réparti entre les VUs (pas besoin de cryptographie).
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let mut current = STATE.load(Ordering::Relaxed);
    loop {
        let next = current
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        match STATE.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return (next >> 33) % 1_000_000_000,
            Err(observed) => current = observed,
        }
    }
}

pub fn render(input: &str, vars: &HashMap<String, String>, vu: u64, iter: u64) -> String {
    variable_regex()
        .replace_all(input, |captures: &regex::Captures<'_>| {
            let name = &captures[1];
            match name {
                "__VU" => vu.to_string(),
                "__ITER" => iter.to_string(),
                "timestamp" => now_ms().to_string(),
                "random" => random_value().to_string(),
                _ => vars
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| captures[0].to_string()),
            }
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_known_and_unknown_variables() {
        let mut vars = HashMap::new();
        vars.insert("id".to_string(), "42".to_string());
        assert_eq!(render("/v1/{{id}}/{{ autre }}", &vars, 3, 7), "/v1/42/{{ autre }}");
        assert_eq!(render("{{__VU}}-{{__ITER}}", &vars, 3, 7), "3-7");
        assert!(render("{{timestamp}}", &vars, 0, 0).parse::<u64>().is_ok());
        let random = render("{{random}}", &vars, 0, 0).parse::<u64>().unwrap();
        assert!(random < 1_000_000_000);
    }
}

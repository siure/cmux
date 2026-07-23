#![cfg_attr(all(test, not(feature = "gtk")), allow(dead_code))]

use crate::config::BrowserSearchSettings;
use url::Url;

#[cfg(any(feature = "gtk", test))]
use serde_json::Value;
#[cfg(any(feature = "gtk", test))]
use std::sync::mpsc;
#[cfg(any(feature = "gtk", test))]
use std::time::Duration;

#[cfg(any(feature = "gtk", test))]
const REMOTE_SUGGESTION_TIMEOUT: Duration = Duration::from_millis(650);
const REMOTE_SUGGESTION_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserOmnibarResolutionKind {
    Url,
    Search,
}

impl BrowserOmnibarResolutionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::Search => "search",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserOmnibarResolution {
    pub url: String,
    pub kind: BrowserOmnibarResolutionKind,
    pub search_engine: Option<String>,
}

pub fn resolve_browser_omnibar_input(
    input: &str,
    settings: &BrowserSearchSettings,
) -> Option<BrowserOmnibarResolution> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(url) = resolve_browser_navigable_url(trimmed) {
        return Some(BrowserOmnibarResolution {
            url,
            kind: BrowserOmnibarResolutionKind::Url,
            search_engine: None,
        });
    }
    let url = browser_search_url(settings, trimmed)?;
    Some(BrowserOmnibarResolution {
        url,
        kind: BrowserOmnibarResolutionKind::Search,
        search_engine: Some(settings.engine.clone()),
    })
}

pub fn resolve_browser_navigable_url(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let bare_host = browser_bare_host_candidate(&lower);
    if lower.starts_with("localhost")
        || lower.starts_with("127.0.0.1")
        || lower.starts_with("[::1]")
        || (bare_host != ".localhost" && bare_host.ends_with(".localhost"))
    {
        return valid_url(&format!("http://{trimmed}"));
    }

    if let Ok(parsed) = Url::parse(trimmed) {
        match parsed.scheme().to_ascii_lowercase().as_str() {
            "http" | "https" => return valid_url(trimmed),
            "file" if parsed.path().starts_with('/') => return Some(parsed.to_string()),
            scheme if browser_dotted_host_with_port_candidate(trimmed, scheme) => {
                return valid_url(&format!("https://{trimmed}"));
            }
            _ => return None,
        }
    }

    if trimmed.contains(':') || trimmed.contains('/') || trimmed.contains('.') {
        return valid_url(&format!("https://{trimmed}"));
    }
    None
}

pub fn browser_search_engine_display_name(engine: &str, custom_name: &str) -> String {
    match engine {
        "google" => "Google",
        "duckduckgo" => "DuckDuckGo",
        "bing" => "Bing",
        "kagi" => "Kagi",
        "startpage" => "Startpage",
        "brave" => "Brave Search",
        "perplexity" => "Perplexity",
        "exa" => "Exa",
        "yahoo" => "Yahoo",
        "ecosia" => "Ecosia",
        "qwant" => "Qwant",
        "mojeek" => "Mojeek",
        "wikipedia" => "Wikipedia",
        "github" => "GitHub",
        "baidu" => "Baidu",
        "yandex" => "Yandex",
        "custom" => {
            let trimmed = custom_name.trim();
            if trimmed.is_empty() {
                "Custom"
            } else {
                return trimmed.to_string();
            }
        }
        _ => "Google",
    }
    .to_string()
}

pub fn browser_search_url(settings: &BrowserSearchSettings, query: &str) -> Option<String> {
    let template = match settings.engine.as_str() {
        "google" => "https://www.google.com/search?q={query}",
        "duckduckgo" => "https://duckduckgo.com/?q={query}",
        "bing" => "https://www.bing.com/search?q={query}",
        "kagi" => "https://kagi.com/search?q={query}",
        "startpage" => "https://www.startpage.com/do/dsearch?q={query}",
        "brave" => "https://search.brave.com/search?q={query}",
        "perplexity" => "https://www.perplexity.ai/search?q={query}",
        "exa" => "https://exa.ai/search?q={query}",
        "yahoo" => "https://search.yahoo.com/search?p={query}",
        "ecosia" => "https://www.ecosia.org/search?q={query}",
        "qwant" => "https://www.qwant.com/?q={query}",
        "mojeek" => "https://www.mojeek.com/search?q={query}",
        "wikipedia" => "https://en.wikipedia.org/w/index.php?search={query}",
        "github" => "https://github.com/search?q={query}",
        "baidu" => "https://www.baidu.com/s?wd={query}",
        "yandex" => "https://yandex.com/search/?text={query}",
        "custom" => settings.custom_url_template.as_str(),
        _ => "https://www.google.com/search?q={query}",
    };
    render_search_url(template, query)
}

pub fn valid_browser_search_engine(value: &str) -> bool {
    matches!(
        value,
        "google"
            | "duckduckgo"
            | "bing"
            | "kagi"
            | "startpage"
            | "brave"
            | "perplexity"
            | "exa"
            | "yahoo"
            | "ecosia"
            | "qwant"
            | "mojeek"
            | "wikipedia"
            | "github"
            | "baidu"
            | "yandex"
            | "custom"
    )
}

pub fn browser_search_engine_supports_remote_suggestions(engine: &str) -> bool {
    matches!(
        engine,
        "google" | "duckduckgo" | "bing" | "kagi" | "startpage"
    )
}

pub fn should_fetch_remote_search_suggestions(
    settings: &BrowserSearchSettings,
    query: &str,
) -> bool {
    let query = query.trim();
    settings.show_search_suggestions
        && query.chars().count() > 1
        && resolve_browser_navigable_url(query).is_none()
        && browser_search_engine_supports_remote_suggestions(&settings.engine)
}

pub fn browser_suggestion_matches(query: &str, url: &str, title: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return true;
    }
    let url = url.trim().to_ascii_lowercase();
    let title = title.trim().to_ascii_lowercase();
    if query.chars().count() == 1 {
        let url = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(&url);
        let url = url.strip_prefix("www.").unwrap_or(url);
        return url.starts_with(&query) || title.starts_with(&query);
    }
    url.contains(&query) || title.contains(&query)
}

#[cfg(any(feature = "gtk", test))]
pub fn fetch_remote_search_suggestions(
    settings: &BrowserSearchSettings,
    query: &str,
) -> Vec<String> {
    let query = query.trim();
    if !should_fetch_remote_search_suggestions(settings, query) {
        return Vec::new();
    }
    if let Some(forced) = forced_remote_suggestions() {
        return forced;
    }
    if settings.engine == "google" {
        return fetch_google_suggestions_with_fallbacks(query);
    }
    fetch_remote_suggestions_for_engine(&settings.engine, query)
}

#[cfg(any(feature = "gtk", test))]
fn fetch_google_suggestions_with_fallbacks(query: &str) -> Vec<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::scope(|scope| {
        for engine in ["google", "duckduckgo", "bing"] {
            let sender = sender.clone();
            scope.spawn(move || {
                let _ = sender.send(fetch_remote_suggestions_for_engine(engine, query));
            });
        }
        drop(sender);
        for _ in 0..3 {
            let Ok(suggestions) = receiver.recv_timeout(REMOTE_SUGGESTION_TIMEOUT) else {
                break;
            };
            if !suggestions.is_empty() {
                return suggestions;
            }
        }
        Vec::new()
    })
}

#[cfg(any(feature = "gtk", test))]
fn fetch_remote_suggestions_for_engine(engine: &str, query: &str) -> Vec<String> {
    let Some(url) = remote_suggestion_url(engine, query) else {
        return Vec::new();
    };
    let client = match reqwest::blocking::Client::builder()
        .timeout(REMOTE_SUGGESTION_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(_) => return Vec::new(),
    };
    let response = match client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Safari/537.36",
        )
        .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
        .send()
    {
        Ok(response) if response.status().is_success() => response,
        _ => return Vec::new(),
    };
    let bytes = match response.bytes() {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };
    match engine {
        "duckduckgo" => parse_duckduckgo_suggestions(&bytes),
        "google" | "bing" | "kagi" | "startpage" => parse_osjson_suggestions(&bytes),
        _ => Vec::new(),
    }
}

#[cfg(any(feature = "gtk", test))]
fn remote_suggestion_url(engine: &str, query: &str) -> Option<Url> {
    let (base, parameter, extra) = match engine {
        "google" => (
            "https://suggestqueries.google.com/complete/search",
            "q",
            Some(("client", "firefox")),
        ),
        "duckduckgo" => ("https://duckduckgo.com/ac/", "q", Some(("type", "list"))),
        "bing" => ("https://www.bing.com/osjson.aspx", "query", None),
        "kagi" => ("https://kagi.com/api/autosuggest", "q", None),
        "startpage" => ("https://www.startpage.com/osuggestions", "q", None),
        _ => return None,
    };
    let mut url = Url::parse(base).ok()?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair(parameter, query);
        if let Some((key, value)) = extra {
            pairs.append_pair(key, value);
        }
    }
    Some(url)
}

#[cfg(any(feature = "gtk", test))]
fn forced_remote_suggestions() -> Option<Vec<String>> {
    let raw = std::env::var("CMUX_UI_TEST_REMOTE_SUGGESTIONS_JSON")
        .or_else(|_| std::env::var("CMUX_BROWSER_REMOTE_SUGGESTIONS_JSON"))
        .ok()?;
    let value = serde_json::from_str::<Value>(&raw).ok()?;
    let suggestions =
        sanitized_remote_suggestions(value.as_array()?.iter().filter_map(Value::as_str));
    (!suggestions.is_empty()).then_some(suggestions)
}

pub fn sanitized_remote_suggestions<'a>(
    suggestions: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut values = Vec::new();
    for suggestion in suggestions {
        let suggestion = suggestion.trim();
        if suggestion.is_empty()
            || values
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(suggestion))
        {
            continue;
        }
        values.push(suggestion.to_string());
        if values.len() >= REMOTE_SUGGESTION_LIMIT {
            break;
        }
    }
    values
}

#[cfg(any(feature = "gtk", test))]
fn parse_osjson_suggestions(data: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return Vec::new();
    };
    let Some(items) = value
        .as_array()
        .and_then(|root| root.get(1))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    sanitized_remote_suggestions(items.iter().filter_map(Value::as_str))
}

#[cfg(any(feature = "gtk", test))]
fn parse_duckduckgo_suggestions(data: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<Value>(data) else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    sanitized_remote_suggestions(
        items
            .iter()
            .filter_map(|item| item.get("phrase").and_then(Value::as_str)),
    )
}

fn browser_bare_host_candidate(input: &str) -> &str {
    let end = input
        .find(|character| matches!(character, ':' | '/' | '?' | '#'))
        .unwrap_or(input.len());
    &input[..end]
}

fn browser_dotted_host_with_port_candidate(input: &str, scheme_candidate: &str) -> bool {
    if !scheme_candidate.contains('.') {
        return false;
    }
    input
        .strip_prefix(scheme_candidate)
        .and_then(|suffix| suffix.strip_prefix(':'))
        .and_then(|suffix| {
            let port = suffix.split(['/', '?', '#']).next().unwrap_or_default();
            (!port.is_empty() && port.chars().all(|character| character.is_ascii_digit()))
                .then_some(())
        })
        .is_some()
}

fn render_search_url(template: &str, query: &str) -> Option<String> {
    let template = template.trim();
    let query = query.trim();
    if template.is_empty() || query.is_empty() {
        return None;
    }
    let encoded = percent_encode_search_query(query);
    let rendered = if template.contains("{query}") || template.contains("%s") {
        template
            .replace("{query}", &encoded)
            .replace("%s", &encoded)
    } else {
        let separator = if template.contains('?') { '&' } else { '?' };
        format!("{template}{separator}q={encoded}")
    };
    valid_url(&rendered)
}

fn percent_encode_search_query(query: &str) -> String {
    let mut encoded = String::with_capacity(query.len());
    for byte in query.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(*byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn valid_url(value: &str) -> Option<String> {
    let parsed = Url::parse(value).ok()?;
    match parsed.scheme() {
        "http" | "https" if parsed.host_str().is_some_and(|host| !host.is_empty()) => {
            Some(parsed.to_string())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(engine: &str) -> BrowserSearchSettings {
        BrowserSearchSettings {
            engine: engine.to_string(),
            custom_name: "Docs".to_string(),
            custom_url_template: "https://docs.example.test/find/{query}".to_string(),
            show_search_suggestions: true,
        }
    }

    #[test]
    fn navigable_url_resolution_matches_browser_omnibar_rules() {
        assert_eq!(
            resolve_browser_navigable_url("example.com/docs"),
            Some("https://example.com/docs".to_string())
        );
        assert_eq!(
            resolve_browser_navigable_url("example.com:8443/path"),
            Some("https://example.com:8443/path".to_string())
        );
        assert_eq!(
            resolve_browser_navigable_url("localhost:3000"),
            Some("http://localhost:3000/".to_string())
        );
        assert_eq!(
            resolve_browser_navigable_url("http://example.com"),
            Some("http://example.com/".to_string())
        );
        assert!(resolve_browser_navigable_url("search terms").is_none());
        assert!(resolve_browser_navigable_url("javascript:alert(1)").is_none());
    }

    #[test]
    fn search_resolution_uses_builtin_and_custom_templates() {
        let google = resolve_browser_omnibar_input("cmux linux port", &settings("google")).unwrap();
        assert_eq!(google.kind, BrowserOmnibarResolutionKind::Search);
        assert_eq!(
            google.url,
            "https://www.google.com/search?q=cmux%20linux%20port"
        );
        let custom = resolve_browser_omnibar_input("profile data", &settings("custom")).unwrap();
        assert_eq!(custom.url, "https://docs.example.test/find/profile%20data");
    }

    #[test]
    fn custom_template_without_placeholder_appends_query_parameter() {
        let mut custom = settings("custom");
        custom.custom_url_template = "https://docs.example.test/search?scope=all".to_string();
        assert_eq!(
            browser_search_url(&custom, "browser state"),
            Some("https://docs.example.test/search?scope=all&q=browser%20state".to_string())
        );
    }

    #[test]
    fn remote_suggestion_support_matches_macos_engines_and_input_intent() {
        let mut google = settings("google");
        assert!(should_fetch_remote_search_suggestions(
            &google,
            "cmux linux"
        ));
        assert!(!should_fetch_remote_search_suggestions(&google, "c"));
        assert!(!should_fetch_remote_search_suggestions(
            &google,
            "example.com/docs"
        ));
        google.show_search_suggestions = false;
        assert!(!should_fetch_remote_search_suggestions(
            &google,
            "cmux linux"
        ));
        assert!(!browser_search_engine_supports_remote_suggestions("github"));
    }

    #[test]
    fn remote_suggestion_parsers_sanitize_and_bound_results() {
        assert_eq!(
            parse_osjson_suggestions(
                br#"["cmux",["cmux linux"," cmux terminal ","CMUX LINUX","", "four", "five", "six", "seven", "eight", "nine"]]"#
            ),
            vec![
                "cmux linux",
                "cmux terminal",
                "four",
                "five",
                "six",
                "seven",
                "eight",
                "nine"
            ]
        );
        assert_eq!(
            parse_duckduckgo_suggestions(
                br#"[{"phrase":"cmux linux"},{"phrase":" cmux app "},{"other":"ignored"}]"#
            ),
            vec!["cmux linux", "cmux app"]
        );
    }

    #[test]
    fn suggestion_matching_uses_prefixes_for_single_character_queries() {
        assert!(browser_suggestion_matches(
            "e",
            "https://www.example.test/docs",
            "Reference"
        ));
        assert!(browser_suggestion_matches(
            "r",
            "https://example.test/docs",
            "Reference"
        ));
        assert!(!browser_suggestion_matches(
            "x",
            "https://example.test/docs",
            "Reference"
        ));
        assert!(browser_suggestion_matches(
            "amp",
            "https://example.test/docs",
            "Reference"
        ));
    }
}

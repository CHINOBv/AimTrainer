//! The HTTP seam shared by the API providers: a [`Transport`] posts JSON and returns the
//! status and body, so request building and reply parsing are tested without a network.
//! Status codes become [`ModelError`]s here, with any key redacted from provider text.

use std::time::Duration;

use aim_core::coach::ModelError;
use serde_json::Value;

use crate::secrets::ApiKey;

/// Longest provider error message kept in a [`ModelError`].
pub const MAX_ERROR_CHARS: usize = 300;

/// A JSON POST. `Debug` hides header values, which carry the API key.
#[derive(Clone, PartialEq)]
pub struct HttpRequest {
    /// Full URL.
    pub url: String,
    /// Headers besides `content-type: application/json`.
    pub headers: Vec<(String, String)>,
    /// The JSON body. It never contains the key.
    pub body: Value,
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(n, _)| n.as_str()).collect();
        f.debug_struct("HttpRequest")
            .field("url", &self.url)
            .field("headers", &names)
            .finish_non_exhaustive()
    }
}

impl HttpRequest {
    /// The value of header `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// What came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Body text.
    pub body: String,
}

/// Why no response arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// The connection or the whole request took longer than allowed.
    Timeout,
    /// Anything else (DNS, refused connection, TLS).
    Failed(String),
}

/// Posts JSON. Blocking: run it off the frame.
pub trait Transport: Send {
    /// Send `request` and return the response, whatever its status.
    fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// A [`Transport`] over `reqwest`'s blocking client with rustls.
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestTransport {
    /// A client with these timeouts. Redirects are not followed, so a key is never
    /// re-sent to another host.
    pub fn new(connect_timeout: Duration, request_timeout: Duration) -> Result<Self, String> {
        reqwest::blocking::Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map(|client| Self { client })
            .map_err(|e| format!("could not create the HTTP client: {e}"))
    }
}

impl Transport for ReqwestTransport {
    fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let mut builder = self
            .client
            .post(&request.url)
            .header("content-type", "application/json")
            .body(request.body.to_string());
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let response = builder.send().map_err(transport_error)?;
        let status = response.status().as_u16();
        let body = response.text().map_err(transport_error)?;
        Ok(HttpResponse { status, body })
    }
}

/// `reqwest` errors name the URL but never the headers, so they carry no key.
fn transport_error(e: reqwest::Error) -> TransportError {
    if e.is_timeout() {
        TransportError::Timeout
    } else {
        TransportError::Failed(e.without_url().to_string())
    }
}

/// Send `request` and return the body of a 2xx response, or the mapped error.
pub fn send(
    transport: &dyn Transport,
    request: &HttpRequest,
    provider: &str,
    key: Option<&ApiKey>,
) -> Result<String, ModelError> {
    match transport.post_json(request) {
        Ok(response) if (200..300).contains(&response.status) => Ok(response.body),
        Ok(response) => Err(status_error(provider, &response, key)),
        Err(TransportError::Timeout) => Err(ModelError::Request(format!(
            "{provider} did not answer in time; try again, raise the timeout, or pick a \
             faster model"
        ))),
        Err(TransportError::Failed(reason)) => Err(ModelError::Request(format!(
            "could not reach {provider}: {}",
            redact(&reason, key)
        ))),
    }
}

/// A non-2xx response as an error the player can act on.
pub fn status_error(provider: &str, response: &HttpResponse, key: Option<&ApiKey>) -> ModelError {
    let status = response.status;
    let detail = provider_message(&response.body)
        .map(|m| format!(": {}", truncate(&redact(&m, key), MAX_ERROR_CHARS)))
        .unwrap_or_default();
    let message = match status {
        // Authentication messages can echo part of the key; keep none of it.
        401 => format!("{provider} rejected the API key (401); check it in the coach settings"),
        403 => format!("{provider} refused access (403); the key lacks permission{detail}"),
        429 => format!("{provider} rate limit reached (429); wait a moment and try again"),
        500..=599 => format!("{provider} is unavailable ({status}); try again later{detail}"),
        _ => format!("{provider} rejected the request ({status}){detail}"),
    };
    ModelError::Request(message)
}

/// The `error.message` (or `message`) field of a JSON error body.
fn provider_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let message = value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .or_else(|| value.get("error").filter(|e| e.is_string()))?;
    message.as_str().map(str::to_owned)
}

/// `text` with every occurrence of the key replaced.
pub fn redact(text: &str, key: Option<&ApiKey>) -> String {
    match key {
        Some(key) if !key.is_empty() => text.replace(key.expose(), "<redacted>"),
        _ => text.to_owned(),
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Plays back one canned result and records every request.
    #[derive(Clone)]
    pub struct FakeTransport {
        pub result: Result<HttpResponse, TransportError>,
        pub seen: Arc<Mutex<Vec<HttpRequest>>>,
    }

    impl FakeTransport {
        pub fn ok(body: &str) -> Self {
            Self::status(200, body)
        }

        pub fn status(status: u16, body: &str) -> Self {
            Self::result(Ok(HttpResponse {
                status,
                body: body.to_owned(),
            }))
        }

        pub fn result(result: Result<HttpResponse, TransportError>) -> Self {
            Self {
                result,
                seen: Arc::default(),
            }
        }

        pub fn last(&self) -> HttpRequest {
            self.seen
                .lock()
                .unwrap()
                .last()
                .cloned()
                .expect("a request was sent")
        }
    }

    impl Transport for FakeTransport {
        fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
            self.seen.lock().unwrap().push(request.clone());
            self.result.clone()
        }
    }

    fn request() -> HttpRequest {
        HttpRequest {
            url: "https://example.test/v1".into(),
            headers: vec![("authorization".into(), "Bearer sk-secret".into())],
            body: Value::Null,
        }
    }

    fn error_text(transport: FakeTransport) -> String {
        let key = ApiKey::new("sk-secret");
        match send(&transport, &request(), "TestAI", Some(&key)) {
            Err(ModelError::Request(text)) => text,
            other => panic!("expected a request error, got {other:?}"),
        }
    }

    #[test]
    fn request_debug_output_hides_header_values() {
        let shown = format!("{:?}", request());
        assert!(!shown.contains("sk-secret"), "{shown}");
        assert!(shown.contains("authorization"));
    }

    #[test]
    fn a_401_names_the_key_without_echoing_any_of_it() {
        let body = r#"{"error":{"message":"Incorrect API key provided: sk-secret"}}"#;
        let text = error_text(FakeTransport::status(401, body));
        assert!(text.contains("401") && text.contains("API key"), "{text}");
        assert!(!text.contains("sk-"), "{text}");
    }

    #[test]
    fn a_429_asks_the_player_to_wait() {
        let text = error_text(FakeTransport::status(
            429,
            r#"{"error":{"message":"slow"}}"#,
        ));
        assert!(
            text.contains("rate limit") && text.contains("429"),
            "{text}"
        );
    }

    #[test]
    fn a_timeout_suggests_what_to_change() {
        let text = error_text(FakeTransport::result(Err(TransportError::Timeout)));
        assert!(text.contains("did not answer in time"), "{text}");
    }

    #[test]
    fn other_errors_keep_the_provider_message_redacted_and_short() {
        let long = format!("bad model sk-secret {}", "x".repeat(1000));
        let body = serde_json::json!({ "error": { "message": long } }).to_string();
        let text = error_text(FakeTransport::status(400, &body));
        assert!(text.contains("(400): bad model <redacted>"), "{text}");
        assert!(text.len() < MAX_ERROR_CHARS + 100, "{}", text.len());
        let text = error_text(FakeTransport::result(Err(TransportError::Failed(
            "refused sk-secret".into(),
        ))));
        assert_eq!(text, "could not reach TestAI: refused <redacted>");
        let text = error_text(FakeTransport::status(503, "<html>down</html>"));
        assert_eq!(text, "TestAI is unavailable (503); try again later");
    }
}

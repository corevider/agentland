//! A dev server as the preview shows it: passed through untouched, with one
//! script added to every page so a person can point at an element and hand it
//! to an agent.
//!
//! The page lives on another origin than the window, and nothing in the window
//! may reach into it. So the core stands in front of the dev server on a port
//! of its own. Every request and answer passes through as it was, except that
//! an HTML page gets a script tag for the picker, served from the same place
//! and so counted as the page's own. The picker tells the window what was
//! picked with a message, the one thing a framed page may send its parent.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

pub const PICKER_PATH: &str = "/__agentland/picker.js";
const PICKER: &str = include_str!("preview_picker.js");
const MOST_BODY_BYTES: usize = 32 * 1024 * 1024;

/// Headers about the connection rather than the content, and the ones that
/// would describe a body the preview may change.
const NOT_PASSED_ON: &[&str] = &[
    "host",
    "connection",
    "keep-alive",
    "proxy-connection",
    "transfer-encoding",
    "upgrade",
    "te",
    "trailer",
    "accept-encoding",
    "content-length",
];
const NOT_PASSED_BACK: &[&str] = &["connection", "keep-alive", "transfer-encoding", "content-length"];

/// The page with the picker's script in it: at the end of the head where there
/// is one, at the very start where there is not.
pub fn with_the_picker(page: &str) -> String {
    let tag = format!(r#"<script src="{PICKER_PATH}"></script>"#);
    // Lower-casing ASCII keeps every byte where it was, so the offset found in
    // the copy is the offset in the page.
    match page.to_ascii_lowercase().find("</head>") {
        Some(at) => format!("{}{tag}{}", &page[..at], &page[at..]),
        None => format!("{tag}{page}"),
    }
}

/// A redirect back to the dev server's own address points into the preview
/// instead; otherwise the first sign-in page takes the preview out of design
/// mode for good. Anywhere else is left alone.
pub fn kept_here(location: &str, upstream: u16) -> String {
    for home in [format!("http://127.0.0.1:{upstream}"), format!("http://localhost:{upstream}")] {
        if let Some(rest) = location.strip_prefix(&home) {
            if rest.is_empty() {
                return "/".to_owned();
            }
            if rest.starts_with('/') || rest.starts_with('?') {
                return rest.to_owned();
            }
        }
    }
    location.to_owned()
}

#[derive(Clone)]
struct Upstream {
    port: u16,
    client: reqwest::Client,
}

/// The previews open so far, by the dev server's port. One each, kept for as
/// long as the core runs: a dev server restarted on the same port is reached
/// through the same preview.
#[derive(Default)]
pub struct Previews {
    open: tokio::sync::Mutex<BTreeMap<u16, u16>>,
}

impl Previews {
    /// The port the preview of a dev server is served on, opened the first
    /// time it is asked for. Loopback only: the preview adds nothing a
    /// neighbour on the network should reach.
    pub async fn open(&self, upstream: u16) -> Result<u16> {
        let mut open = self.open.lock().await;
        if let Some(port) = open.get(&upstream) {
            return Ok(*port);
        }

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .context("no port was free for a preview")?;
        let port = listener.local_addr()?.port();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let app = Router::new()
            .route(PICKER_PATH, get(the_picker))
            .fallback(pass_through)
            .with_state(Upstream { port: upstream, client });

        tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, app).await {
                tracing::warn!(%error, upstream, "a preview stopped");
            }
        });

        open.insert(upstream, port);
        Ok(port)
    }
}

async fn the_picker() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        PICKER,
    )
}

async fn pass_through(State(upstream): State<Upstream>, request: Request) -> Response {
    match forward(&upstream, request).await {
        Ok(answer) => answer,
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            format!("the dev server on port {} did not answer: {error:#}", upstream.port),
        )
            .into_response(),
    }
}

async fn forward(upstream: &Upstream, request: Request) -> Result<Response> {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path_and_query().map(|held| held.as_str()).unwrap_or("/");
    let body = axum::body::to_bytes(body, MOST_BODY_BYTES).await?;

    let mut asked = upstream
        .client
        .request(parts.method.clone(), format!("http://127.0.0.1:{}{path}", upstream.port))
        .body(body);
    for (name, value) in &parts.headers {
        if !NOT_PASSED_ON.contains(&name.as_str()) {
            asked = asked.header(name, value);
        }
    }
    // Told it is being reached on its own address, which is where it believes
    // it lives: some dev servers turn away a Host they do not know.
    asked = asked.header(header::HOST, format!("127.0.0.1:{}", upstream.port));

    let answer = asked.send().await?;
    let status = answer.status();
    let mut headers = HeaderMap::new();
    for (name, value) in answer.headers() {
        if NOT_PASSED_BACK.contains(&name.as_str()) {
            continue;
        }
        if name == header::LOCATION {
            if let Some(moved) = value
                .to_str()
                .ok()
                .and_then(|location| HeaderValue::from_str(&kept_here(location, upstream.port)).ok())
            {
                headers.append(name.clone(), moved);
                continue;
            }
        }
        headers.append(name.clone(), value.clone());
    }

    // Only a page that arrived as plain text can take the script; one that
    // arrived compressed anyway goes through as it came.
    let is_page = headers
        .get(header::CONTENT_TYPE)
        .and_then(|kind| kind.to_str().ok())
        .is_some_and(|kind| kind.starts_with("text/html"));
    let body = if is_page && !headers.contains_key(header::CONTENT_ENCODING) {
        let page = answer.bytes().await?;
        Body::from(with_the_picker(&String::from_utf8_lossy(&page)))
    } else {
        Body::from_stream(answer.bytes_stream())
    };

    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_goes_at_the_end_of_the_head() {
        assert_eq!(
            with_the_picker("<html><head><title>x</title></HEAD><body>hi</body></html>"),
            format!(r#"<html><head><title>x</title><script src="{PICKER_PATH}"></script></HEAD><body>hi</body></html>"#)
        );
    }

    #[test]
    fn a_page_with_no_head_gets_it_first() {
        assert_eq!(
            with_the_picker("<p>hi</p>"),
            format!(r#"<script src="{PICKER_PATH}"></script><p>hi</p>"#)
        );
    }

    #[test]
    fn a_redirect_home_stays_in_the_preview() {
        assert_eq!(kept_here("http://127.0.0.1:5173/login?next=%2F", 5173), "/login?next=%2F");
        assert_eq!(kept_here("http://localhost:5173", 5173), "/");
        assert_eq!(kept_here("http://127.0.0.1:51730/", 5173), "http://127.0.0.1:51730/", "another port is another server");
        assert_eq!(kept_here("https://accounts.example.com/", 5173), "https://accounts.example.com/");
        assert_eq!(kept_here("/relative", 5173), "/relative");
    }

    #[tokio::test]
    async fn a_page_passes_through_with_the_picker_and_nothing_else_is_touched() {
        let upstream = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let upstream_port = upstream.local_addr().unwrap().port();
        let home = format!("http://127.0.0.1:{upstream_port}/");
        let app = Router::new()
            .route("/", get(|| async { axum::response::Html("<html><head></head><body>shop</body></html>") }))
            .route(
                "/api/items",
                axum::routing::post(|body: String| async move {
                    ([(header::CONTENT_TYPE, "application/json")], format!("{{\"got\":{body}}}"))
                }),
            )
            .route(
                "/old",
                get(move || {
                    let home = home.clone();
                    async move { axum::response::Redirect::temporary(&home) }
                }),
            );
        tokio::spawn(async move { axum::serve(upstream, app).await.unwrap() });

        let previews = Previews::default();
        let port = previews.open(upstream_port).await.unwrap();
        assert_eq!(previews.open(upstream_port).await.unwrap(), port, "one preview per dev server");

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");

        let page = client.get(format!("{base}/")).send().await.unwrap().text().await.unwrap();
        assert!(page.contains(PICKER_PATH), "the page carries the picker: {page}");
        assert!(page.contains("<body>shop</body>"));

        let answer = client.post(format!("{base}/api/items")).body("[1,2]").send().await.unwrap();
        assert_eq!(answer.headers()[header::CONTENT_TYPE], "application/json");
        assert_eq!(answer.text().await.unwrap(), r#"{"got":[1,2]}"#);

        let moved = client.get(format!("{base}/old")).send().await.unwrap();
        assert_eq!(moved.headers()[header::LOCATION], "/");

        let picker = client.get(format!("{base}{PICKER_PATH}")).send().await.unwrap();
        assert!(picker.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/javascript"));
        assert!(picker.text().await.unwrap().contains("agentland"));
    }
}

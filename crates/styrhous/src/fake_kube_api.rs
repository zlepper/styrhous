//! A scripted fake Kubernetes API server for tests.
//!
//! The server speaks just enough HTTP/1.1 for `kube::Client`: it reads each
//! request (head and body), records it, asks the script for a [`Reply`] and
//! answers it, immediately, after a delay, or never ([`Reply::Stalled`]).
//! Connections are served concurrently. Dropping the [`FakeKubeApi`] stops the
//! server and every connection it is serving.
#![cfg(test)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::{JoinHandle, JoinSet};

/// A request as the fake server received it.
#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub method: String,
    /// The request target: the path and, when present, the query string.
    pub target: String,
    /// The target without its query string.
    pub path: String,
    /// The `accept` header, or an empty string.
    pub accept: String,
    pub body: String,
    /// How many requests arrived before this one (from 0).
    pub index: usize,
    /// How many requests to the same `METHOD path` route arrived before this one.
    pub nth: usize,
}

impl Request {
    /// `METHOD path`, the key scripts usually match on.
    pub(crate) fn route(&self) -> String {
        format!("{} {}", self.method, self.path)
    }
}

/// What the fake server answers to a request.
#[derive(Clone, Debug)]
pub(crate) enum Reply {
    /// A complete response.
    Response {
        status: u16,
        content_type: &'static str,
        body: Vec<u8>,
    },
    /// The inner reply, after the delay.
    Delayed(Duration, Box<Reply>),
    /// Accept the request and never answer, keeping the connection open.
    Stalled,
}

impl Reply {
    /// A JSON response with `status`.
    pub(crate) fn json(status: u16, body: impl Into<String>) -> Self {
        Self::Response {
            status,
            content_type: "application/json",
            body: body.into().into_bytes(),
        }
    }

    /// A `200 OK` JSON response.
    pub(crate) fn ok_json(body: impl Into<String>) -> Self {
        Self::json(200, body)
    }

    /// A `200 OK` plain text response, such as a Pod log.
    pub(crate) fn text(body: impl Into<Vec<u8>>) -> Self {
        Self::Response {
            status: 200,
            content_type: "text/plain",
            body: body.into(),
        }
    }

    /// This reply, sent only after `delay`.
    pub(crate) fn after(self, delay: Duration) -> Self {
        Self::Delayed(delay, Box::new(self))
    }
}

type Script = dyn Fn(&Request) -> Reply + Send + Sync;

#[derive(Default)]
struct Shared {
    requests: Mutex<Vec<Request>>,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

/// A running fake Kubernetes API server.
pub(crate) struct FakeKubeApi {
    address: SocketAddr,
    shared: Arc<Shared>,
    server: JoinHandle<()>,
}

impl FakeKubeApi {
    /// Start a server answering every request with `script`.
    pub(crate) async fn start(script: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("the fake API server binds");
        let address = listener.local_addr().expect("the listener has an address");
        let shared = Arc::new(Shared::default());
        let script: Arc<Script> = Arc::new(script);
        let serving = Arc::clone(&shared);
        let server = tokio::spawn(async move {
            // Dropping the set (when the server task is aborted) aborts every connection.
            let mut connections = JoinSet::new();
            while let Ok((socket, _)) = listener.accept().await {
                connections.spawn(serve_connection(
                    socket,
                    Arc::clone(&script),
                    Arc::clone(&serving),
                ));
            }
        });
        Self {
            address,
            shared,
            server,
        }
    }

    /// Start a server answering the `n`th request with `replies[n]` (and `404` once they run out).
    pub(crate) async fn start_sequence(replies: Vec<Reply>) -> Self {
        Self::start(move |request| {
            replies.get(request.index).cloned().unwrap_or_else(|| {
                Reply::json(404, r#"{"kind":"Status","code":404,"reason":"NotFound"}"#)
            })
        })
        .await
    }

    /// Start a server that accepts requests and never answers them.
    pub(crate) async fn start_stalled() -> Self {
        Self::start(|_| Reply::Stalled).await
    }

    /// A `kube` configuration pointing at this server.
    pub(crate) fn config(&self) -> kube::Config {
        kube::Config::new(
            format!("http://{}", self.address)
                .parse()
                .expect("the test server URL is valid"),
        )
    }

    pub(crate) fn client(&self) -> kube::Client {
        kube::Client::try_from(self.config()).expect("the test client builds")
    }

    /// Every request received so far, in arrival order.
    pub(crate) fn requests(&self) -> Vec<Request> {
        self.shared.requests.lock().unwrap().clone()
    }

    /// The target (path and query) of every request received so far.
    pub(crate) fn targets(&self) -> Vec<String> {
        self.requests()
            .into_iter()
            .map(|request| request.target)
            .collect()
    }

    /// The path of every request received so far.
    pub(crate) fn paths(&self) -> Vec<String> {
        self.requests()
            .into_iter()
            .map(|request| request.path)
            .collect()
    }

    /// The most requests that were waiting for their reply at the same time.
    pub(crate) fn max_in_flight(&self) -> usize {
        self.shared.max_in_flight.load(Ordering::SeqCst)
    }
}

impl Drop for FakeKubeApi {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream, shared: &Shared) -> Option<Request> {
    let mut received = Vec::new();
    let mut buffer = [0_u8; 4_096];
    let (head_end, length) = loop {
        if let Some(head_end) = received.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&received[..head_end]).to_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .and_then(|length| length.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if received.len() >= head_end + 4 + length {
                break (head_end + 4, length);
            }
        }
        match socket.read(&mut buffer).await {
            Ok(0) | Err(_) => return None,
            Ok(read) => received.extend_from_slice(&buffer[..read]),
        }
    };
    let head = String::from_utf8_lossy(&received[..head_end]).into_owned();
    let mut start_line = head.split_whitespace();
    let method = start_line.next().unwrap_or_default().to_owned();
    let target = start_line.next().unwrap_or_default().to_owned();
    let path = target.split('?').next().unwrap_or_default().to_owned();
    let accept = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("accept")
                .then(|| value.trim().to_owned())
        })
        .unwrap_or_default();
    let body = String::from_utf8_lossy(&received[head_end..head_end + length]).into_owned();
    let mut requests = shared.requests.lock().unwrap();
    let mut request = Request {
        method,
        target,
        path,
        accept,
        body,
        index: requests.len(),
        nth: 0,
    };
    request.nth = requests
        .iter()
        .filter(|seen| seen.method == request.method && seen.path == request.path)
        .count();
    requests.push(request.clone());
    Some(request)
}

async fn serve_connection(
    mut socket: tokio::net::TcpStream,
    script: Arc<Script>,
    shared: Arc<Shared>,
) {
    let Some(request) = read_request(&mut socket, &shared).await else {
        return;
    };
    let in_flight = shared.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
    shared.max_in_flight.fetch_max(in_flight, Ordering::SeqCst);
    let mut reply = script(&request);
    while let Reply::Delayed(delay, inner) = reply {
        tokio::time::sleep(delay).await;
        reply = *inner;
    }
    match reply {
        Reply::Stalled => std::future::pending::<()>().await,
        Reply::Response {
            status,
            content_type,
            body,
        } => {
            shared.in_flight.fetch_sub(1, Ordering::SeqCst);
            let head = format!(
                "HTTP/1.1 {status} Scripted\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&body).await;
            let _ = socket.shutdown().await;
        }
        Reply::Delayed(..) => unreachable!("delays are resolved above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn get(client: &kube::Client, path: &str) -> u16 {
        let request = http::Request::get(path)
            .body(kube::client::Body::empty())
            .unwrap();
        client.send(request).await.unwrap().status().as_u16()
    }

    #[tokio::test]
    async fn requests_are_recorded_and_sequenced_and_replies_can_be_delayed() {
        let api = FakeKubeApi::start_sequence(vec![
            Reply::ok_json("{}"),
            Reply::json(418, "{}").after(Duration::from_millis(50)),
        ])
        .await;
        let client = api.client();

        assert_eq!(get(&client, "/a?x=1").await, 200);
        let started = std::time::Instant::now();
        assert_eq!(get(&client, "/a").await, 418);
        assert!(started.elapsed() >= Duration::from_millis(50));
        assert_eq!(get(&client, "/b").await, 404, "replies ran out");

        let requests = api.requests();
        assert_eq!(api.targets(), ["/a?x=1", "/a", "/b"]);
        assert_eq!(api.paths(), ["/a", "/a", "/b"]);
        assert_eq!(
            requests
                .iter()
                .map(|r| (r.index, r.nth))
                .collect::<Vec<_>>(),
            [(0, 0), (1, 1), (2, 0)]
        );
    }

    #[tokio::test]
    async fn a_stalled_reply_keeps_the_request_waiting() {
        let api = FakeKubeApi::start_stalled().await;
        let client = api.client();
        let waited = tokio::time::timeout(Duration::from_millis(100), get(&client, "/x")).await;
        assert!(waited.is_err());
        assert_eq!(api.max_in_flight(), 1);
    }
}

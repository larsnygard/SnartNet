use serde_json::json;
use snartnet_sdk::{Client, Command, DaemonPaths, Error, RuntimeMetadata, API_VERSION};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
struct Fixture {
    _root: tempfile::TempDir,
    paths: DaemonPaths,
    requests: Arc<Mutex<Vec<String>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(replies: Vec<Option<String>>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let paths =
            DaemonPaths::from_data_dir(Some(root.path().join("data").to_str().unwrap())).unwrap();
        fs::create_dir_all(paths.runtime_dir()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let metadata = RuntimeMetadata {
            api_version: API_VERSION,
            address: listener.local_addr().unwrap(),
            pid: 1,
            started_at: 1,
        };
        fs::write(paths.metadata(), serde_json::to_vec(&metadata).unwrap()).unwrap();
        fs::write(paths.token(), TOKEN).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let capture = requests.clone();
        let worker = thread::spawn(move || {
            for reply in replies {
                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                let (mut socket, _) = loop {
                    match listener.accept() {
                        Ok(socket) => break socket,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(e) => panic!("missing expected request: {e}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                assert!(request
                    .to_lowercase()
                    .contains(&format!("authorization: bearer {}", TOKEN.to_lowercase())));
                let length = request
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                request.push_str(&String::from_utf8(body).unwrap());
                capture.lock().unwrap().push(request);
                if let Some(reply) = reply {
                    socket.write_all(reply.as_bytes()).unwrap();
                }
            }
        });
        Self {
            _root: root,
            paths,
            requests,
            worker: Some(worker),
        }
    }
    fn client(&self) -> Client {
        Client::new(self.paths.clone()).unwrap()
    }
    fn finish(mut self) -> Vec<String> {
        self.worker.take().unwrap().join().unwrap();
        self.requests.lock().unwrap().clone()
    }
}
fn response(status: u16, body: &str, content_type: &str) -> Option<String> {
    // Chunked framing deliberately exercises the HTTP client rather than an ad-hoc parser.
    Some(format!("HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",body.len()))
}
fn health(version: u64) -> Option<String> {
    response(
        200,
        &json!({"apiVersion":version,"revision":0,"syncMode":"balanced","futureField":true})
            .to_string(),
        "application/json",
    )
}
fn snapshot(revision: u64) -> Option<String> {
    response(200, &json!({"apiVersion":1,"revision":revision,"state":{"posts":[],"contacts":[],"threads":[],"futureField":true}}).to_string(), "application/json")
}

#[test]
fn authenticated_chunked_reads_accept_additive_fields() {
    let fixture = Fixture::new(vec![health(1), snapshot(7)]);
    let client = fixture.client();
    assert_eq!(client.health().unwrap().api_version, 1);
    assert_eq!(client.snapshot().unwrap().revision, 7);
    assert_eq!(fixture.finish().len(), 2);
}

#[test]
fn incompatible_health_prevents_a_write_and_auto_start() {
    let fixture = Fixture::new(vec![health(2), health(2)]);
    let client = fixture.client();
    assert!(matches!(
        client.command(&Command::Post {
            content: "hello".into()
        }),
        Err(Error::Incompatible { found: 2 })
    ));
    assert!(matches!(
        client.ensure_running(std::path::Path::new("nonexistent")),
        Err(Error::Incompatible { found: 2 })
    ));
    assert!(fixture
        .finish()
        .iter()
        .all(|request| request.starts_with("GET")));
}

#[test]
fn authentication_failure_is_terminal() {
    let fixture = Fixture::new(vec![response(401, "{}", "application/json")]);
    assert!(matches!(
        fixture
            .client()
            .ensure_running(std::path::Path::new("nonexistent")),
        Err(Error::Unauthorized)
    ));
    assert_eq!(fixture.finish().len(), 1);
}

#[test]
fn server_validation_errors_reach_the_frontend() {
    let fixture = Fixture::new(vec![
        health(1),
        response(
            400,
            r#"{"error":"Create your profile first"}"#,
            "application/json",
        ),
    ]);
    let error = fixture
        .client()
        .command(&Command::Post {
            content: "hello".into(),
        })
        .unwrap_err();
    assert!(matches!(&error, Error::Http { status: 400, .. }));
    assert!(error.to_string().contains("Create your profile first"));
    assert_eq!(fixture.finish().len(), 2);
}

#[test]
fn redirect_is_not_followed_and_snapshot_version_is_checked() {
    let redirect = Some("HTTP/1.1 302 Found\r\nLocation: http://192.0.2.1/collect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into());
    let fixture = Fixture::new(vec![
        redirect,
        response(
            200,
            r#"{"apiVersion":99,"revision":0,"state":{}}"#,
            "application/json",
        ),
    ]);
    let client = fixture.client();
    assert!(matches!(
        client.health(),
        Err(Error::Http { status: 302, .. })
    ));
    assert!(matches!(
        client.snapshot(),
        Err(Error::Incompatible { found: 99 })
    ));
    assert_eq!(fixture.finish().len(), 2);
}

#[test]
fn reads_retry_but_uncertain_writes_are_never_replayed() {
    let fixture = Fixture::new(vec![None, health(1), health(1), None]);
    let client = fixture.client();
    client.health().unwrap();
    assert!(matches!(
        client.command(&Command::Post {
            content: "once".into()
        }),
        Err(Error::Transport)
    ));
    let requests = fixture.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.starts_with("POST"))
            .count(),
        1
    );
    assert!(requests.last().unwrap().contains("\"content\":\"once\""));
}

#[test]
fn sse_recovers_full_snapshot_after_eof_and_revision_reset() {
    let event = "data: {\"apiVersion\":1,\"revision\":4,\"kind\":\"snapshot\"}\r\n\r\n";
    let fixture = Fixture::new(vec![
        health(1),
        response(200, event, "text/event-stream"),
        snapshot(4),
        snapshot(5),
        health(1),
        response(200, ": heartbeat\n\n", "text/event-stream"),
        snapshot(0),
    ]);
    let mut subscription = fixture.client().subscribe();
    assert_eq!(subscription.next_snapshot().unwrap().revision, 4);
    assert_eq!(subscription.next_snapshot().unwrap().revision, 5);
    assert_eq!(subscription.next_snapshot().unwrap().revision, 0);
    let requests = fixture.finish();
    assert!(requests[1].starts_with("GET /v1/events"));
    assert!(requests[2].starts_with("GET /v1/snapshot"));
}

#[test]
fn runtime_metadata_cannot_redirect_credentials_off_loopback() {
    let fixture = Fixture::new(vec![]);
    let metadata = RuntimeMetadata {
        api_version: 1,
        address: "192.0.2.1:80".parse().unwrap(),
        pid: 1,
        started_at: 1,
    };
    fs::write(
        fixture.paths.metadata(),
        serde_json::to_vec(&metadata).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        fixture.client().health(),
        Err(Error::InvalidRuntime)
    ));
    fixture.finish();
}

#[test]
fn healthy_daemon_does_not_spawn_and_unknown_commands_are_rejected() {
    let fixture = Fixture::new(vec![health(1)]);
    fixture
        .client()
        .ensure_running(std::path::Path::new("nonexistent"))
        .unwrap();
    fixture.finish();
    assert!(serde_json::from_value::<Command>(json!({"op":"exportKeys"})).is_err());
    assert!(serde_json::from_value::<Command>(
        json!({"op":"post","content":"hello","unexpected":true})
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn auto_start_waits_for_runtime_readiness() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(vec![health(1)]);
    let metadata = fs::read(fixture.paths.metadata()).unwrap();
    fs::remove_file(fixture.paths.metadata()).unwrap();
    fs::create_dir_all(fixture.paths.data_dir()).unwrap();
    let executable = fixture._root.path().join("start-fixture");
    fs::write(&executable, include_str!("start-fixture.sh")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let marker = fixture._root.path().join("started");
    let paths = fixture.paths.clone();
    let publish = thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "SDK did not launch executable"
            );
            thread::sleep(Duration::from_millis(10));
        }
        fs::write(paths.metadata(), metadata).unwrap();
    });
    fixture.client().ensure_running(&executable).unwrap();
    publish.join().unwrap();
    fixture.finish();
}

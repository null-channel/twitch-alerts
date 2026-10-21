//! Runs the real executable against local OAuth and EventSub servers.
//! No Twitch account, API key, or external service is used.
use std::{
    collections::HashMap,
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{Form, Query, State},
    http::{HeaderMap, StatusCode, Uri},
    response::Redirect,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::{net::TcpListener, task::JoinSet};
use tokio_tungstenite::tungstenite::Message;
use twitch_api::twitch_oauth2::CsrfToken;

const TIMESTAMP: &str = "2026-10-18T12:00:00Z";
const CLIENT_SECRET: &str = "local+client-secret&/=";

#[derive(Default)]
struct MockTwitch {
    initial_token_expired: bool,
    code_exchanges: usize,
    refreshes: usize,
    subscriptions: Vec<(String, String)>,
}

type SharedTwitch = Arc<Mutex<MockTwitch>>;

async fn authorize(Query(query): Query<HashMap<String, String>>) -> Redirect {
    assert_eq!(query["client_id"], "local-client");
    assert_eq!(query["response_type"], "code");
    assert!(query["scope"].contains("moderator:read:followers"));
    assert!(query["scope"].contains("channel:read:subscriptions"));
    let mut callback = url::Url::parse(&query["redirect_uri"]).unwrap();
    callback
        .query_pairs_mut()
        .append_pair("code", "local-authorization-code")
        .append_pair("state", &query["state"]);
    Redirect::to(callback.as_str())
}

async fn exchange(
    State(state): State<SharedTwitch>,
    uri: Uri,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    assert!(
        uri.query().is_none(),
        "credentials must be sent in the form body"
    );
    assert_eq!(form["client_id"], "local-client");
    assert_eq!(form["client_secret"], CLIENT_SECRET);
    let mut state = state.lock().unwrap();
    let generation = match form["grant_type"].as_str() {
        "authorization_code" => {
            assert_eq!(form["code"], "local-authorization-code");
            state.code_exchanges += 1;
            1
        }
        "refresh_token" => {
            assert_eq!(
                form["refresh_token"],
                format!("local-refresh-{}", state.refreshes + 1)
            );
            state.refreshes += 1;
            state.refreshes + 1
        }
        other => panic!("unexpected grant type: {}", other),
    };
    Json(json!({
        "access_token": format!("local-access-{generation}"),
        "refresh_token": format!("local-refresh-{generation}"),
        // The first refreshed token schedules background maintenance in two seconds.
        "expires_in": if generation == 2 { 302 } else { 14400 },
        "token_type": "bearer",
        "scope": ["moderator:read:followers", "channel:read:subscriptions"]
    }))
}

async fn validate(
    State(state): State<SharedTwitch>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let state = state.lock().unwrap();
    let auth = headers["authorization"].to_str().unwrap();
    if auth.ends_with("local-access-1") && state.initial_token_expired {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":401,"message":"invalid access token"})),
        );
    }
    assert!(auth.ends_with(&format!("local-access-{}", state.refreshes + 1)));
    (
        StatusCode::OK,
        Json(json!({
            "client_id":"local-client", "login":"local_broadcaster", "user_id":"123",
            "scopes":["moderator:read:followers", "channel:read:subscriptions"],
            "expires_in": if state.refreshes == 1 { 1 } else { 14400 }
        })),
    )
}

async fn subscribe(
    State(state): State<SharedTwitch>,
    headers: HeaderMap,
    Json(mut subscription): Json<Value>,
) -> (StatusCode, Json<Value>) {
    assert_eq!(headers["client-id"], "local-client");
    assert_eq!(subscription["transport"]["method"], "websocket");
    let auth = headers["authorization"].to_str().unwrap().to_owned();
    let mut state = state.lock().unwrap();
    state
        .subscriptions
        .push((subscription["type"].as_str().unwrap().into(), auth));
    subscription["id"] = json!(format!("local-subscription-{}", state.subscriptions.len()));
    subscription["status"] = json!("enabled");
    subscription["cost"] = json!(0);
    subscription["created_at"] = json!(TIMESTAMP);
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "data":[subscription], "total":1, "total_cost":0, "max_total_cost":10000
        })),
    )
}

struct LocalServices {
    http: SocketAddr,
    websocket: SocketAddr,
    state: SharedTwitch,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl LocalServices {
    async fn start() -> Self {
        let state = SharedTwitch::default();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/oauth2/authorize", get(authorize))
            .route("/oauth2/token", post(exchange))
            .route("/oauth2/validate", get(validate))
            .route("/helix/eventsub/subscriptions", post(subscribe))
            .with_state(state.clone());
        let http_task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let websocket = listener.local_addr().unwrap();
        let websocket_task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    connection = listener.accept() => {
                        let (stream, _) = connection.unwrap();
                        connections.spawn(async move {
                            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                            socket.send(Message::Text(json!({
                                "metadata":{"message_id":"welcome", "message_type":"session_welcome", "message_timestamp":TIMESTAMP},
                                "payload":{"session":{"id":"local-session", "status":"connected", "keepalive_timeout_seconds":10,
                                    "reconnect_url":null, "connected_at":TIMESTAMP}}
                            }).to_string())).await.unwrap();
                            while let Some(message) = socket.next().await {
                                if message.is_err() { break; }
                            }
                        });
                    }
                    Some(result) = connections.join_next() => { result.unwrap(); }
                }
            }
        });
        Self {
            http,
            websocket,
            state,
            tasks: vec![http_task, websocket_task],
        }
    }
}

impl Drop for LocalServices {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let name = CsrfToken::new_random().secret().replace('/', "_");
        let path = std::env::temp_dir().join(format!("alerts-local-{name}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RunningApp {
    child: Child,
    log: PathBuf,
}

impl RunningApp {
    fn start(directory: &Path, services: &LocalServices, label: &str, args: &[String]) -> Self {
        let log = directory.join(format!("{label}.log"));
        let output = fs::File::create(&log).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_twitch_alerts"))
            .current_dir(directory)
            .env_clear()
            .env("TWITCH_CLIENT_ID", "local-client")
            .env("TWITCH_CLIENT_SECRET", CLIENT_SECRET)
            .env("TWITCH_TOKEN_FILE", directory.join("twitch-tokens.json"))
            .env(
                "TWITCH_OAUTH2_URL",
                format!("http://{}/oauth2/", services.http),
            )
            .env(
                "TWITCH_HELIX_URL",
                format!("http://{}/helix/", services.http),
            )
            .env(
                "TWITCH_EVENTSUB_WEBSOCKET_URL",
                format!("ws://{}/", services.websocket),
            )
            .env("OPENAI_KEY", "local-unused-key")
            .env("CHANNEL_ID", "123")
            .env("DB_PATH", directory.join("alerts.db"))
            .env(
                "TWITCH_ALERTS_LOG",
                directory.join(format!("{label}-tracing.log")),
            )
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output))
            .args(args)
            .spawn()
            .unwrap();
        Self { child, log }
    }

    fn output(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }

    fn ensure_running(&mut self) {
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "application exited: {}",
            self.output()
        );
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        self.child.wait().unwrap();
    }
}

impl Drop for RunningApp {
    fn drop(&mut self) {
        self.stop();
    }
}

fn unused_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_for(app: &mut RunningApp, mut ready: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ready() {
        app.ensure_running();
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out: {}",
            app.output()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn saved_tokens(directory: &Path) -> Value {
    serde_json::from_slice(&fs::read(directory.join("twitch-tokens.json")).unwrap()).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn local_authorization_renewal_and_restart() {
    let directory = TestDirectory::new();
    let services = LocalServices::start().await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let callback = format!("127.0.0.1:{}", unused_port());
    let mut app = RunningApp::start(
        &directory.0,
        &services,
        "authorize",
        &[
            "authorize".into(),
            "--listen-addr".into(),
            callback.clone(),
            "--redirect-url".into(),
            format!("http://{callback}/auth/twitch/callback"),
        ],
    );
    let log = app.log.clone();
    let auth_prefix = format!("http://{}/oauth2/authorize?", services.http);
    wait_for(&mut app, || {
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .any(|line| line.starts_with(&auth_prefix))
    })
    .await;
    // A forged callback must neither finish setup nor write a token file.
    let rejected = client
        .get(format!(
            "http://{callback}/auth/twitch/callback?code=forged&state=wrong"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    assert!(!directory.0.join("twitch-tokens.json").exists());
    let authorization_url = app
        .output()
        .lines()
        .find(|line| line.starts_with(&auth_prefix))
        .unwrap()
        .to_owned();
    let approved = client
        .get(authorization_url)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert!(approved.text().await.unwrap().contains("Twitch connected"));
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(status) = app.child.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(status.success(), "authorization failed: {}", app.output());
    assert_eq!(
        saved_tokens(&directory.0)["refresh_token"],
        "local-refresh-1"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(directory.0.join("twitch-tokens.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    services.state.lock().unwrap().initial_token_expired = true;
    let http_port = unused_port();
    let ws_port = unused_port();
    let args = vec![
        "server".into(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        http_port.to_string(),
        "--ws-host".into(),
        "127.0.0.1".into(),
        "--ws-port".into(),
        ws_port.to_string(),
    ];
    app = RunningApp::start(&directory.0, &services, "server", &args);
    wait_for(&mut app, || {
        services.state.lock().unwrap().subscriptions.len() == 5
    })
    .await;
    assert_eq!(
        saved_tokens(&directory.0)["refresh_token"],
        "local-refresh-2"
    );
    for route in [
        "/",
        "/admin",
        "/assets/css/alert.css",
        "/assets/websocket.js",
    ] {
        let body = client
            .get(format!("http://127.0.0.1:{http_port}{route}"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(!body.is_empty());
        if route == "/" || route == "/admin" {
            assert!(body.contains(&format!("ws://127.0.0.1:{ws_port}/")));
        }
    }
    let (mut browser, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{ws_port}/"))
        .await
        .unwrap();
    browser
        .send(Message::Text("local-browser-check".into()))
        .await
        .unwrap();
    assert_eq!(
        browser.next().await.unwrap().unwrap(),
        Message::Text("local-browser-check".into())
    );
    browser.close(None).await.unwrap();
    // Verify background renewal while the process remains alive.
    wait_for(&mut app, || {
        saved_tokens(&directory.0)["refresh_token"] == "local-refresh-3"
    })
    .await;
    assert_eq!(services.state.lock().unwrap().refreshes, 2);
    app.stop();
    // Recover the saved rotated token after a hard stop without authorizing again.
    app = RunningApp::start(&directory.0, &services, "restarted", &args);
    wait_for(&mut app, || {
        services.state.lock().unwrap().subscriptions.len() == 10
    })
    .await;
    let state = services.state.lock().unwrap();
    assert_eq!(state.code_exchanges, 1);
    assert_eq!(state.refreshes, 2);
    let expected = [
        "channel.follow",
        "channel.subscribe",
        "channel.raid",
        "channel.subscription.message",
        "channel.subscription.gift",
    ];
    for (index, (subscription, auth)) in state.subscriptions.iter().enumerate() {
        assert_eq!(subscription, expected[index % 5]);
        assert_eq!(
            auth,
            if index < 5 {
                "Bearer local-access-2"
            } else {
                "Bearer local-access-3"
            }
        );
    }
}

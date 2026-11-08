use super::*;
use axum::http::{Request, Response};
use serde_json::json;
use std::{collections::VecDeque, path::PathBuf, sync::Mutex};

type OAuthResponse = Result<Response<Vec<u8>>, std::io::Error>;

#[derive(Default)]
struct MockClient {
    responses: Mutex<VecDeque<OAuthResponse>>,
    requests: Mutex<Vec<Request<Vec<u8>>>>,
}

impl MockClient {
    fn reply(&self, status: u16, body: serde_json::Value) {
        self.responses
            .lock()
            .unwrap()
            .push_back(Ok(Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .body(serde_json::to_vec(&body).unwrap())
                .unwrap()));
    }
}

impl Client for MockClient {
    type Error = std::io::Error;

    fn req(
        &self,
        request: Request<Vec<u8>>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = OAuthResponse> + Send + '_>> {
        self.requests.lock().unwrap().push(request);
        let result = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected OAuth request");
        Box::pin(async { result })
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let name = CsrfToken::new_random().secret().replace('/', "_");
        let path = std::env::temp_dir().join(format!("twitch-auth-{name}"));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn file(&self) -> PathBuf {
        self.0.join("twitch-tokens.json")
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn old_token() -> UserToken {
    UserToken::from_existing_unchecked(
        "old-access",
        Some("old+refresh&/=".into()),
        "client-id",
        Some("client-secret".into()),
        "broadcaster".into(),
        "123".into(),
        Some(required_scopes()),
        Some(Duration::ZERO),
    )
}

fn options(file: PathBuf) -> TwitchBotArgs {
    TwitchBotArgs {
        oauth: OAuthArgs {
            twitch_client_id: Some("client-id".into()),
            twitch_client_secret: Some("client-secret".parse().unwrap()),
            token_file: file,
        },
        access_token: None,
        channel_login: None,
        channel_id: None,
        oauth2_service_url: None,
        oauth2_service_key: None,
        oauth2_service_pointer: None,
        oauth2_service_refresh: None,
    }
}

fn manager(file: PathBuf) -> TokenManager {
    TokenManager {
        token: Arc::new(RwLock::new(old_token())),
        client: http_client().unwrap(),
        opts: options(file),
        managed: true,
        dirty: false,
    }
}

fn valid(expires_in: u64) -> serde_json::Value {
    json!({"client_id":"client-id", "login":"broadcaster", "user_id":"123",
        "scopes":["moderator:read:followers", "channel:read:subscriptions"], "expires_in":expires_in})
}

fn renewed() -> serde_json::Value {
    json!({"access_token":"new-access", "refresh_token":"new-refresh",
        "expires_in":14400, "token_type":"bearer", "scope":["moderator:read:followers", "channel:read:subscriptions"]})
}

fn invalid() -> serde_json::Value {
    json!({"status":401, "message":"invalid access token"})
}

fn saved(file: &Path) -> StoredTokens {
    serde_json::from_slice(&fs::read(file).unwrap()).unwrap()
}

#[tokio::test]
async fn expired_token_refreshes_and_rotated_credentials_survive_restart() {
    let dir = TestDirectory::new();
    let mut manager = manager(dir.file());
    save_token(&dir.file(), &old_token()).unwrap();
    let client = OAuthClient(MockClient::default());
    client.0.reply(401, invalid());
    client.0.reply(200, renewed());
    manager.maintain_with(&client).await.unwrap();
    assert_eq!(
        manager.token.read().await.access_token.secret(),
        "new-access"
    );
    assert_eq!(saved(&dir.file()).refresh_token, "new-refresh");

    let restarted = saved(&dir.file()).into_token(&manager.opts.oauth).unwrap();
    *manager.token.write().await = restarted;
    client.0.reply(200, valid(14000));
    manager.maintain_with(&client).await.unwrap();
    assert_eq!(client.0.requests.lock().unwrap().len(), 3);
    assert!(manager.token.read().await.expires_in() > Duration::from_secs(13000));
    let requests = client.0.requests.lock().unwrap();
    let refresh = &requests[1];
    assert!(refresh.uri().query().is_none());
    assert_eq!(
        refresh.headers()["content-type"],
        "application/x-www-form-urlencoded"
    );
    let fields: std::collections::HashMap<_, _> = url::form_urlencoded::parse(refresh.body())
        .into_owned()
        .collect();
    assert_eq!(fields["grant_type"], "refresh_token");
    assert_eq!(fields["client_secret"], "client-secret");
    assert_eq!(fields["refresh_token"], "old+refresh&/=");
}

#[tokio::test]
async fn refresh_outage_keeps_credentials_and_next_attempt_succeeds() {
    let dir = TestDirectory::new();
    let mut manager = manager(dir.file());
    save_token(&dir.file(), &old_token()).unwrap();
    let client = MockClient::default();
    client.reply(200, valid(60));
    client.reply(
        503,
        json!({"status":503, "message":"temporarily unavailable"}),
    );
    assert!(manager.maintain_with(&client).await.is_err());
    assert_eq!(
        manager
            .token
            .read()
            .await
            .refresh_token
            .as_ref()
            .unwrap()
            .secret(),
        "old+refresh&/="
    );
    assert_eq!(saved(&dir.file()).access_token, "old-access");
    client.reply(200, valid(60));
    client.reply(200, renewed());
    manager.maintain_with(&client).await.unwrap();
    assert_eq!(saved(&dir.file()).refresh_token, "new-refresh");
}

#[tokio::test]
async fn temporary_validation_failure_does_not_refresh_or_modify_credentials() {
    let dir = TestDirectory::new();
    let mut manager = manager(dir.file());
    let client = MockClient::default();
    client
        .responses
        .lock()
        .unwrap()
        .push_back(Err(std::io::Error::other("network unavailable")));
    assert!(manager.maintain_with(&client).await.is_err());
    assert_eq!(client.requests.lock().unwrap().len(), 1);
    assert_eq!(
        manager.token.read().await.access_token.secret(),
        "old-access"
    );
    assert!(!dir.file().exists());
}

#[tokio::test]
async fn rejected_refresh_requires_authorization() {
    let dir = TestDirectory::new();
    let mut manager = manager(dir.file());
    let client = MockClient::default();
    client.reply(401, invalid());
    client.reply(
        400,
        json!({"status":400, "message":"invalid refresh token"}),
    );
    let error = manager.maintain_with(&client).await.unwrap_err();
    assert!(error.downcast_ref::<AuthorizationRequired>().is_some());
    assert_eq!(
        manager.token.read().await.access_token.secret(),
        "old-access"
    );
}

#[tokio::test]
async fn failed_disk_write_keeps_rotated_token_in_memory_and_retries_saving_it() {
    let dir = TestDirectory::new();
    let mut manager = manager(dir.file());
    fs::create_dir(dir.file()).unwrap(); // Renaming a file over a directory must fail.
    let client = MockClient::default();
    client.reply(401, invalid());
    client.reply(200, renewed());
    assert!(manager.maintain_with(&client).await.is_err());
    assert!(manager.dirty);
    assert_eq!(
        manager
            .token
            .read()
            .await
            .refresh_token
            .as_ref()
            .unwrap()
            .secret(),
        "new-refresh"
    );
    fs::remove_dir(dir.file()).unwrap();
    client.reply(200, valid(14000));
    manager.maintain_with(&client).await.unwrap();
    assert!(!manager.dirty);
    assert_eq!(saved(&dir.file()).refresh_token, "new-refresh");
    assert_eq!(client.requests.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn changed_identity_or_missing_permissions_stops_maintenance() {
    let dir = TestDirectory::new();
    let mut manager = manager(dir.file());
    for field in ["user_id", "client_id", "scopes"] {
        let client = MockClient::default();
        let mut response = valid(14400);
        response[field] = if field == "scopes" {
            json!([])
        } else {
            json!("different")
        };
        client.reply(200, response);
        let error = manager.maintain_with(&client).await.unwrap_err();
        assert!(error.downcast_ref::<AuthorizationRequired>().is_some());
    }
}

#[test]
fn token_file_is_private_and_does_not_store_the_client_secret() {
    let dir = TestDirectory::new();
    save_token(&dir.file(), &old_token()).unwrap();
    assert!(!fs::read_to_string(dir.file())
        .unwrap()
        .contains("client-secret"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(dir.file()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let mut opts = options(dir.file()).oauth;
    opts.twitch_client_id = Some("different-client".into());
    assert!(saved(&dir.file()).into_token(&opts).is_err());
}

#[tokio::test]
async fn authorization_checks_state_saves_tokens_and_cannot_be_replayed() {
    let dir = TestDirectory::new();
    let mut builder = UserTokenBuilder::new(
        "client-id",
        "client-secret",
        "http://localhost:3000/auth/twitch/callback"
            .parse()
            .unwrap(),
    )
    .set_scopes(required_scopes());
    let (url, csrf) = builder.generate_url();
    assert!(url
        .query_pairs()
        .any(|(key, value)| key == "response_type" && value == "code"));
    let builder = tokio::sync::Mutex::new(Some(builder));
    let (sender, mut receiver) = mpsc::channel(1);
    let client = OAuthClient(MockClient::default());
    for state in [None, Some("forged-state".into())] {
        let callback = Callback {
            code: Some("code".into()),
            state,
            error: None,
        };
        let response =
            authorization_callback(&builder, &client, &dir.file(), &sender, callback).await;
        assert_eq!(response.0, StatusCode::BAD_REQUEST);
        assert!(builder.lock().await.is_some());
        assert!(receiver.try_recv().is_err());
        assert!(!dir.file().exists());
    }
    client.0.reply(200, renewed());
    client.0.reply(200, valid(14400));
    let callback = Callback {
        code: Some("one-time-code".into()),
        state: Some(csrf.secret().into()),
        error: None,
    };
    let response = authorization_callback(&builder, &client, &dir.file(), &sender, callback).await;
    assert_eq!(response.0, StatusCode::OK);
    receiver.recv().await.unwrap().unwrap();
    assert_eq!(saved(&dir.file()).refresh_token, "new-refresh");
    let callback = Callback {
        code: Some("one-time-code".into()),
        state: Some(csrf.secret().into()),
        error: None,
    };
    assert_eq!(
        authorization_callback(&builder, &client, &dir.file(), &sender, callback)
            .await
            .0,
        StatusCode::CONFLICT
    );
    let requests = client.0.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].uri().query().is_none());
    let fields: std::collections::HashMap<_, _> = url::form_urlencoded::parse(requests[0].body())
        .into_owned()
        .collect();
    assert_eq!(fields["grant_type"], "authorization_code");
    assert_eq!(fields["code"], "one-time-code");
}

#[tokio::test]
async fn denied_authorization_does_not_save_credentials() {
    let dir = TestDirectory::new();
    let mut builder = UserTokenBuilder::new(
        "client-id",
        "client-secret",
        "http://localhost:3000/auth/twitch/callback"
            .parse()
            .unwrap(),
    );
    let (_, csrf) = builder.generate_url();
    let builder = tokio::sync::Mutex::new(Some(builder));
    let (sender, mut receiver) = mpsc::channel(1);
    let callback = Callback {
        code: None,
        state: Some(csrf.secret().into()),
        error: Some("access_denied".into()),
    };
    assert_eq!(
        authorization_callback(
            &builder,
            &MockClient::default(),
            &dir.file(),
            &sender,
            callback
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert!(receiver.recv().await.unwrap().is_err());
    assert!(!dir.file().exists());
}

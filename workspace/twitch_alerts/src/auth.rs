use std::{
    fs::{self, OpenOptions},
    future::IntoFuture,
    io::Write,
    path::Path,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use axum::{extract::Query, http::StatusCode, routing::get, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use twitch_api::twitch_oauth2::{
    client::Client,
    tokens::errors::{RefreshTokenError, ValidationError},
    ClientSecret, CsrfToken, RequestParseError, Scope, TwitchToken, UserToken, UserTokenBuilder,
};

use crate::opts::{AuthorizeArgs, OAuthArgs, TwitchBotArgs};

const REFRESH_MARGIN: Duration = Duration::from_secs(300);
const VALIDATE_INTERVAL: Duration = Duration::from_secs(3600);
const RETRY_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct OAuthClient<C>(C);

impl<C: Client> Client for OAuthClient<C> {
    type Error = C::Error;

    fn req(
        &self,
        mut request: axum::http::Request<Vec<u8>>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<axum::http::Response<Vec<u8>>, Self::Error>>
                + Send
                + '_,
        >,
    > {
        // This twitch_oauth2 version puts POST credentials in URL parameters.
        // Send them in the form body, as Twitch documents, to keep them out of URLs.
        if request.method() == axum::http::Method::POST {
            if let Some(query) = request.uri().query() {
                let body = query.as_bytes().to_vec();
                let mut parts = request.uri().clone().into_parts();
                parts.path_and_query =
                    Some(request.uri().path().parse().expect("valid request path"));
                *request.uri_mut() =
                    axum::http::Uri::from_parts(parts).expect("valid OAuth request URI");
                *request.body_mut() = body;
                request.headers_mut().insert(
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("application/x-www-form-urlencoded"),
                );
            }
        }
        self.0.req(request)
    }
}

pub fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("twitch-alerts/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("build Twitch HTTP client")
}

fn credentials(opts: &OAuthArgs) -> anyhow::Result<(&str, ClientSecret)> {
    let id = opts
        .twitch_client_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .context("set TWITCH_CLIENT_ID to your Twitch application's client ID")?;
    let secret = opts
        .twitch_client_secret
        .as_ref()
        .map(|s| s.secret())
        .filter(|s| !s.is_empty())
        .context("set TWITCH_CLIENT_SECRET to the same Twitch application's client secret")?;
    Ok((id, secret.into()))
}

fn required_scopes() -> Vec<Scope> {
    vec![
        Scope::ModeratorReadFollowers,
        Scope::ChannelReadSubscriptions,
    ]
}

fn check_token(token: &UserToken, expected: &UserToken) -> anyhow::Result<()> {
    if token.client_id() != expected.client_id() || token.user_id != expected.user_id {
        return Err(AuthorizationRequired)
            .context("Twitch token belongs to a different application or user");
    }
    Ok(())
}

fn check_scopes(token: &UserToken) -> anyhow::Result<()> {
    if !required_scopes()
        .iter()
        .all(|scope| token.scopes().contains(scope))
    {
        return Err(AuthorizationRequired)
            .context("Twitch permissions are missing; authorize again as the broadcaster");
    }
    Ok(())
}

// Never derive Debug: this representation contains plaintext credentials.
#[derive(Serialize, Deserialize)]
struct StoredTokens {
    client_id: String,
    access_token: String,
    refresh_token: String,
    login: String,
    user_id: String,
    scopes: Vec<Scope>,
}

impl StoredTokens {
    fn from_token(token: &UserToken) -> anyhow::Result<Self> {
        Ok(Self {
            client_id: token.client_id().to_string(),
            access_token: token.access_token.secret().to_owned(),
            refresh_token: token
                .refresh_token
                .as_ref()
                .context("Twitch did not return a refresh token; authorize again")?
                .secret()
                .to_owned(),
            login: token.login.to_string(),
            user_id: token.user_id.to_string(),
            scopes: token.scopes().to_vec(),
        })
    }

    fn into_token(self, opts: &OAuthArgs) -> anyhow::Result<UserToken> {
        let (client_id, secret) = credentials(opts)?;
        anyhow::ensure!(client_id == self.client_id, "TWITCH_CLIENT_ID does not match the saved tokens; authorize with this application again");
        // An expired placeholder forces validation before the saved credentials are used.
        Ok(UserToken::from_existing_unchecked(
            self.access_token,
            Some(self.refresh_token.into()),
            self.client_id,
            Some(secret),
            self.login.into(),
            self.user_id.into(),
            Some(self.scopes),
            Some(Duration::ZERO),
        ))
    }
}

fn save_token(path: &Path, token: &UserToken) -> anyhow::Result<()> {
    let data = serde_json::to_vec(&StoredTokens::from_token(token)?)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).context("create Twitch token directory")?;
    let suffix = CsrfToken::new_random()
        .secret()
        .replace('/', "_")
        .replace('+', "-");
    let temporary = path.with_extension(format!("{suffix}.tmp"));
    let result = (|| -> anyhow::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .context("create private Twitch token file")?;
        file.write_all(&data).context("write Twitch tokens")?;
        file.sync_all().context("sync Twitch tokens")?;
        fs::rename(&temporary, path).context("replace Twitch token file")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[derive(Debug)]
struct AuthorizationRequired;

impl std::fmt::Display for AuthorizationRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Twitch authorization is no longer valid; check TWITCH_CLIENT_SECRET and run `twitch_alerts authorize` again")
    }
}

impl std::error::Error for AuthorizationRequired {}

async fn validate<C: Client>(
    client: &C,
    token: &UserToken,
    secret: Option<ClientSecret>,
) -> anyhow::Result<Option<UserToken>> {
    match token.access_token.validate_token(client).await {
        Ok(validated) => {
            let updated = UserToken::new(
                token.access_token.clone(),
                token.refresh_token.clone(),
                validated,
                secret,
            )?;
            check_token(&updated, token)?;
            Ok(Some(updated))
        }
        Err(ValidationError::NotAuthorized) => Ok(None),
        // Don't include remote response bodies in errors or logs.
        Err(_) => anyhow::bail!("Twitch token validation failed; will retry"),
    }
}

async fn refresh<C: Client>(client: &C, token: &UserToken) -> anyhow::Result<UserToken> {
    // The library consumes the refresh token even on failure. Work on a clone so
    // a temporary outage cannot destroy the credentials needed for another try.
    let mut updated = token.clone();
    match updated.refresh_token(client).await {
        Ok(()) => {
            if updated.refresh_token.is_none() {
                updated.refresh_token = token.refresh_token.clone();
            }
            Ok(updated)
        }
        Err(RefreshTokenError::RequestParseError(RequestParseError::TwitchError(error)))
            if matches!(error.status.as_u16(), 400 | 401) =>
        {
            Err(AuthorizationRequired.into())
        }
        Err(RefreshTokenError::RequestParseError(RequestParseError::Other(status)))
            if matches!(status.as_u16(), 400 | 401) =>
        {
            Err(AuthorizationRequired.into())
        }
        Err(RefreshTokenError::NoRefreshToken | RefreshTokenError::NoClientSecretFound) => {
            Err(AuthorizationRequired.into())
        }
        Err(_) => anyhow::bail!("Twitch token renewal failed; will retry"),
    }
}

pub struct TokenManager {
    pub token: Arc<RwLock<UserToken>>,
    client: reqwest::Client,
    opts: TwitchBotArgs,
    managed: bool,
    dirty: bool,
}

impl TokenManager {
    pub async fn load(client: reqwest::Client, opts: &TwitchBotArgs) -> anyhow::Result<Self> {
        let managed = opts.access_token.is_none() && opts.oauth2_service_url.is_none();
        let token = if managed {
            let data = fs::read(&opts.oauth.token_file)
                .context("read Twitch token file; run `twitch_alerts authorize` once first")?;
            serde_json::from_slice::<StoredTokens>(&data)
                .context("read saved Twitch credentials")?
                .into_token(&opts.oauth)?
        } else {
            if opts.access_token.is_some() {
                tracing::warn!("ACCESS_TOKEN cannot be renewed automatically; use `twitch_alerts authorize` for unattended operation");
            }
            crate::utils::get_access_token(&client, opts).await?
        };
        let mut manager = Self {
            token: Arc::new(RwLock::new(token)),
            client,
            opts: opts.clone(),
            managed,
            dirty: false,
        };
        if managed {
            manager.retry_maintenance().await?;
        }
        Ok(manager)
    }

    async fn persist(&mut self) -> anyhow::Result<()> {
        if self.dirty {
            save_token(&self.opts.oauth.token_file, &*self.token.read().await)?;
            self.dirty = false;
        }
        Ok(())
    }

    async fn maintain(&mut self) -> anyhow::Result<()> {
        self.maintain_with(&OAuthClient(self.client.clone())).await
    }

    async fn maintain_with<C: Client>(&mut self, client: &C) -> anyhow::Result<()> {
        self.persist().await?;
        let current = self.token.read().await.clone();
        let secret = if self.managed {
            Some(credentials(&self.opts.oauth)?.1)
        } else {
            None
        };
        let validated = validate(client, &current, secret).await?;
        let margin = self.refresh_margin();
        if let Some(ref token) = validated {
            if self.managed {
                check_scopes(token)?;
            }
            if token.expires_in() > margin {
                *self.token.write().await = token.clone();
                return Ok(());
            }
        }
        let updated = if self.managed {
            refresh(client, validated.as_ref().unwrap_or(&current)).await?
        } else if self.opts.oauth2_service_url.is_some() {
            let token = crate::utils::get_access_token(&self.client, &self.opts).await?;
            check_token(&token, &current)?;
            anyhow::ensure!(
                token.expires_in() > margin,
                "OAuth service returned a token that is still expiring; will retry"
            );
            token
        } else {
            return Err(AuthorizationRequired.into());
        };
        *self.token.write().await = updated;
        self.dirty = self.managed;
        // Save the rotated refresh token immediately, before another network request.
        // If writing fails, keep the new credentials in memory and retry the write.
        self.persist().await?;
        tracing::info!("Twitch access token renewed");
        Ok(())
    }

    fn refresh_margin(&self) -> Duration {
        if self.managed {
            REFRESH_MARGIN
        } else {
            Duration::from_secs(self.opts.oauth2_service_refresh.unwrap_or(30))
        }
    }

    async fn retry_maintenance(&mut self) -> anyhow::Result<()> {
        while let Err(error) = self.maintain().await {
            if error.downcast_ref::<AuthorizationRequired>().is_some() {
                return Err(error);
            }
            tracing::warn!(error = %error, "Twitch credential maintenance failed; retrying in 30 seconds");
            tokio::time::sleep(RETRY_INTERVAL).await;
        }
        Ok(())
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            let delay = if self.dirty {
                RETRY_INTERVAL
            } else {
                self.token
                    .read()
                    .await
                    .expires_in()
                    .saturating_sub(self.refresh_margin())
                    .min(VALIDATE_INTERVAL)
                    .max(Duration::from_secs(1))
            };
            tokio::time::sleep(delay).await;
            self.retry_maintenance().await?;
        }
    }
}

#[derive(Deserialize)]
struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn authorization_callback<C: Client>(
    builder: &tokio::sync::Mutex<Option<UserTokenBuilder>>,
    client: &C,
    token_file: &Path,
    sender: &mpsc::Sender<anyhow::Result<()>>,
    callback: Callback,
) -> (StatusCode, &'static str) {
    let mut pending = builder.lock().await;
    let Some(flow) = pending.as_ref() else {
        return (StatusCode::CONFLICT, "Authorization has already completed.");
    };
    if !callback
        .state
        .as_deref()
        .is_some_and(|state| flow.csrf_is_valid(state))
    {
        return (
            StatusCode::BAD_REQUEST,
            "Invalid authorization state. Use the link printed in the terminal.",
        );
    }
    if callback.error.is_some() {
        pending.take();
        let _ = sender
            .send(Err(anyhow::anyhow!(
                "Twitch authorization was denied; run authorize again"
            )))
            .await;
        return (StatusCode::BAD_REQUEST, "Twitch authorization was denied.");
    }
    let Some(code) = callback.code.as_deref().filter(|code| !code.is_empty()) else {
        return (StatusCode::BAD_REQUEST, "Missing authorization code.");
    };
    let flow = pending.take().expect("authorization flow checked above");
    drop(pending);
    let result = async {
        let token = flow.get_user_token(client, callback.state.as_deref().unwrap_or_default(), code).await
            .map_err(|_| anyhow::anyhow!("Twitch authorization exchange failed; check the client credentials and registered redirect URL, then authorize again"))?;
        check_scopes(&token)?;
        save_token(token_file, &token)?;
        Ok(())
    }.await;
    let response = if result.is_ok() {
        (StatusCode::OK, "Twitch connected. Credentials saved; you can close this tab and start the alerts server.")
    } else {
        (
            StatusCode::BAD_GATEWAY,
            "Could not save Twitch authorization. Check the terminal and run authorize again.",
        )
    };
    let _ = sender.send(result).await;
    response
}

pub async fn authorize(opts: &AuthorizeArgs) -> anyhow::Result<()> {
    let (client_id, secret) = credentials(&opts.oauth)?;
    anyhow::ensure!(
        matches!(opts.redirect_url.scheme(), "http" | "https")
            && opts.redirect_url.query().is_none()
            && opts.redirect_url.fragment().is_none(),
        "redirect URL must be an HTTP(S) URL without query parameters or a fragment"
    );
    let mut builder = UserTokenBuilder::new(client_id, secret, opts.redirect_url.clone())
        .set_scopes(required_scopes())
        .force_verify(true);
    let (url, _) = builder.generate_url();
    let builder = Arc::new(tokio::sync::Mutex::new(Some(builder)));
    let (sender, mut receiver) = mpsc::channel(1);
    let listener = tokio::net::TcpListener::bind(opts.listen_addr)
        .await
        .context("bind Twitch authorization callback listener")?;
    let client = OAuthClient(http_client()?);
    let token_file = opts.oauth.token_file.clone();
    let app = Router::new().route(
        opts.redirect_url.path(),
        get(move |Query(callback): Query<Callback>| {
            let builder = builder.clone();
            let sender = sender.clone();
            let client = client.clone();
            let token_file = token_file.clone();
            async move {
                authorization_callback(&builder, &client, &token_file, &sender, callback).await
            }
        }),
    );
    println!("Open this URL in your browser and authorize as the broadcaster:\n\n{url}\n\nWaiting for the callback at {} (listening on {}).", opts.redirect_url, opts.listen_addr);
    let (shutdown, stopped) = tokio::sync::oneshot::channel();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .into_future();
    tokio::pin!(server);
    let outcome = tokio::select! {
        result = &mut server => return result.context("serve Twitch authorization callback"),
        result = tokio::time::timeout(Duration::from_secs(600), receiver.recv()) =>
            result.context("Twitch authorization timed out after 10 minutes; run authorize again")?
                .context("Twitch authorization callback closed")?,
    };
    let _ = shutdown.send(());
    server.await.context("stop Twitch authorization callback")?;
    outcome?;
    println!("Saved Twitch credentials to {}. Start the server with the same TWITCH_CLIENT_ID, TWITCH_CLIENT_SECRET, and TWITCH_TOKEN_FILE; remove ACCESS_TOKEN and OAUTH2_SERVICE_URL to use these credentials.", opts.oauth.token_file.display());
    Ok(())
}

#[cfg(test)]
mod tests;

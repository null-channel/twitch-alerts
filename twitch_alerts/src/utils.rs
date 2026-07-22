use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Context;
use twitch_api::twitch_oauth2::UserToken;

pub fn install_utils() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv(); //ignore error
    install_tracing().context("install tracing")?;
    Ok(())
}

/// Log file path: `NULL_TWITCH_LOG` env, else `null_twitch.log` in the current directory.
/// Logs go to stderr and to this file (append mode); use the file when a TUI owns the terminal.
fn install_tracing() -> anyhow::Result<()> {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let log_path: PathBuf = std::env::var_os("NULL_TWITCH_LOG")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("null_twitch.log"));

    if let Some(dir) = log_path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).with_context(|| format!("create log dir {}", dir.display()))?;
        }
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("open log file {}", log_path.display()))?;

    let fmt_stderr = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_target(true);

    let fmt_file = fmt::layer()
        .with_writer(Mutex::new(log_file))
        .with_file(true)
        .with_line_number(true)
        .with_target(true)
        .with_ansi(false);

    #[rustfmt::skip]
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .map(|f| {
            f.add_directive("hyper=error".parse().expect("could not make directive"))
                .add_directive("h2=error".parse().expect("could not make directive"))
                .add_directive("rustls=error".parse().expect("could not make directive"))
                .add_directive("tungstenite=error".parse().expect("could not make directive"))
                .add_directive("retainer=info".parse().expect("could not make directive"))
                .add_directive("want=info".parse().expect("could not make directive"))
                .add_directive("reqwest=info".parse().expect("could not make directive"))
                .add_directive("mio=info".parse().expect("could not make directive"))
            //.add_directive("tower_http=error".parse().unwrap())
        })
        .expect("could not make filter layer");

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_stderr)
        .with(fmt_file)
        .with(ErrorLayer::default())
        .init();

    tracing::info!(path = %log_path.display(), "tracing file sink enabled");
    Ok(())
}

#[tracing::instrument(skip(client, token))]
pub async fn make_token<'a>(
    client: &'a impl twitch_api::twitch_oauth2::client::Client,
    token: impl Into<twitch_api::twitch_oauth2::AccessToken>,
) -> anyhow::Result<UserToken> {
    UserToken::from_existing(client, token.into(), None, None)
        .await
        .map_err(Into::into)
}

#[tracing::instrument(skip(client, opts))]
pub async fn get_access_token(
    client: &reqwest::Client,
    opts: &crate::opts::TwitchBotArgs,
) -> anyhow::Result<UserToken> {
    if let Some(ref access_token) = opts.access_token {
        make_token(client, access_token.to_string()).await
    } else if let (Some(ref oauth_service_url), Some(ref pointer)) =
        (&opts.oauth2_service_url, &opts.oauth2_service_pointer)
    {
        tracing::info!(
            "using oauth service on `{}` to get oauth token",
            oauth_service_url
        );

        let mut request = client.get(oauth_service_url.as_str());
        if let Some(ref key) = opts.oauth2_service_key {
            request = request.bearer_auth(key.secret());
        }
        let request = request.build()?;
        tracing::debug!("request: {:?}", request);

        match client.execute(request).await {
            Ok(response)
                if !(response.status().is_client_error()
                    || response.status().is_server_error()) =>
            {
                let service_response: serde_json::Value = response.json().await?;
                make_token(
                    client,
                    service_response
                        .pointer(pointer)
                        .ok_or_else(|| {
                            anyhow::format_err!("could not get a field on `{}`", pointer)
                        })?
                        .as_str()
                        .ok_or_else(|| anyhow::format_err!("token is not a string"))?
                        .to_string(),
                )
                .await
            }
            Ok(response_error) => {
                let status = response_error.status();
                let error = response_error.text().await?;
                anyhow::bail!(
                    "oauth service returned error code: {} with body: {:?}",
                    status,
                    error
                );
            }
            Err(e) => Err(e).context(format!("calling oauth service on")),
        }
    } else {
        panic!("got empty vals for token cli group")
    }
}

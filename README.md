# Twitch alerts and Bruce

Two independent applications live in this Rust workspace:

| Package | Responsibility |
| --- | --- |
| `twitch_alerts` | Twitch EventSub, generated alert messages, browser overlay, and admin UI |
| `bruce` | Games played with Twitch chat, currently Wordle in a terminal UI |
| `twitch_common` | Twitch transport and event types; opt into `chat` or `eventsub` |
| `sql_common` | SQLite connection setup and migration execution |
| `wordle_word` | Bruce's word-list dependency, maintained as a Git submodule |

Neither application depends on the other. The browser UI belongs to alerts; the
terminal UI belongs to Bruce. Each application embeds its own database migrations.
Bruce also embeds its UI templates and uses the word-list crate's embedded data,
so it does not need sibling source directories at runtime. Alerts ships its static
browser assets separately.

Use Rust 1.95 or newer and initialize the word-list submodule before building:

```bash
git submodule update --init --recursive
cargo build --locked -p twitch_alerts
cargo build --locked -p bruce
```

On Debian/Ubuntu, native dependencies require `pkg-config` and `libssl-dev`.

## Run alerts

From the workspace root, configure Twitch authorization as described below and
`OPENAI_KEY` in your environment or `.env`. See
[twitch_alerts/.env.example](twitch_alerts/.env.example) for available settings.

```bash
cargo run -p twitch_alerts -- server --channel-login your_channel
```

Alert stories use `gpt-6-luna` by default. Override it with `OPENAI_MODEL` or
`--openai-model`. The selected model must support Chat Completions with
`reasoning_effort=none`; this keeps these short alert requests fast and avoids
reasoning-token costs. Responses are capped at 128 completion tokens, and the
story prompt asks for at most 27 words. See the current
[OpenAI model specifications and pricing](https://developers.openai.com/api/docs/models/gpt-6-luna).
The Fly configuration explicitly sets `OPENAI_MODEL=gpt-6-luna`; the API key goes
in the `OPENAI_KEY` Fly secret.

HTTP defaults to `0.0.0.0:8080`, and the WebSocket listener uses port `9000`.
The overlay is at `/` and the admin UI at `/admin`. `WEBSOCKET_HOST` is the hostname
browsers use to reach the WebSocket listener. `HTTP_PORT` and `WEBSOCKET_PORT`
configure the two ports independently. Browser WebSockets use `ws` on HTTP pages
and `wss` on HTTPS pages.

The database defaults to `alerts.db` in the current directory. Set `DB_PATH` or
`--db-path` to use an existing database. For a copied binary, also set
`FRONTEND_ASSETS` to the copied `twitch_alerts/src/frontend/assets` directory.

## Twitch authorization and automatic renewal

Register a **confidential** application in the
[Twitch developer console](https://dev.twitch.tv/console/apps), or use your existing
application. Add this exact OAuth redirect URL to its settings:
`http://localhost:3000/auth/twitch/callback`.

Set these values in the environment or the workspace root `.env`:

```dotenv
TWITCH_CLIENT_ID=your_application_client_id
TWITCH_CLIENT_SECRET=your_application_client_secret
TWITCH_TOKEN_FILE=twitch-tokens.json
```

Stop the alerts server, then run the one-time setup:

```bash
cargo run -p twitch_alerts -- authorize
```

Open the URL printed in the terminal and approve access **as the broadcaster**
whose alerts you want to monitor. The command requests `moderator:read:followers`,
`channel:read:subscriptions`, and `bits:read`, checks the callback's random state,
saves the access and refresh tokens, and exits. It does not need `OPENAI_KEY`.
Tokens saved before `bits:read` was added keep working; the server logs that cheer
alerts are off until you run `authorize` again.

Remove any old `ACCESS_TOKEN` or `OAUTH2_SERVICE_URL` setting from both your shell
and `.env`, then start alerts normally. Explicit legacy settings take precedence
over the saved tokens. Keep `TWITCH_CLIENT_ID`, `TWITCH_CLIENT_SECRET`, and
`TWITCH_TOKEN_FILE` configured for the server. The client ID and secret must belong
to the application that issued the tokens.

The server validates authorization at startup and at least hourly, renews access
tokens five minutes before expiry, and immediately saves any rotated refresh
token. An expired saved access token is recovered automatically at startup.
Temporary network failures retry every 30 seconds. If Twitch revokes the grant or
rejects the refresh credentials, the server exits with instructions to authorize
again. See Twitch's [refresh](https://dev.twitch.tv/docs/authentication/refresh-tokens/)
and [validation](https://dev.twitch.tv/docs/authentication/validate-tokens/) documentation.

The token file contains secrets, is written atomically with mode `0600` on Unix,
and is ignored by Git. The client secret stays in your environment. Use a private
directory on persistent storage, run one alerts instance per token file, and keep
the same file across restarts and deployments. For the Docker image, set
`TWITCH_TOKEN_FILE=/data/twitch-tokens.json` and mount the same `/data` volume for
both authorization and the server. Litestream backs up the database separately;
it does not back up this token file.

### One-time setup on a remote server

Use SSH forwarding so the callback in your local browser reaches the temporary
listener on the webserver. From your computer:

```bash
ssh -L 3000:127.0.0.1:3000 user@your-webserver
```

In that SSH session, configure the environment and run `twitch_alerts authorize`
(or the Cargo command above), then open its printed URL in your local browser.
The default listener binds only to `127.0.0.1:3000` and waits up to ten minutes.
After it succeeds, start the normal server; SSH forwarding is only needed during
this initial authorization.

Alternatively, set `TWITCH_REDIRECT_URL=https://your-domain/auth/twitch/callback`
and register that exact URL with Twitch. During setup, proxy that path to
`127.0.0.1:3000`. `authorize --listen-addr` changes the temporary bind address;
`authorize --redirect-url` changes the externally visible callback URL. With
Docker, publish the callback port on loopback and pass
`--listen-addr 0.0.0.0:3000` inside the container. No callback listener is needed
for subsequent token renewal.

### Existing token configurations

`ACCESS_TOKEN` / `--access-token` still supports a standalone user token, but it
cannot renew without refresh credentials. Use the authorization command above to
switch to automatic renewal; an implicit-flow token cannot be converted into a
refreshable token.

`OAUTH2_SERVICE_URL` / `--oauth2-service-url` still supports an external token
provider. The server fetches a new token near expiry or after failed validation.
`OAUTH2_SERVICE_KEY` supplies an optional bearer key,
`OAUTH2_SERVICE_POINTER` selects the token from the JSON response (default
`/access_token`), and `OAUTH2_SERVICE_REFRESH` controls the renewal margin in
seconds (default `30`).

## Run Bruce

Configure `TC_USERNAME`, `TC_PASSWORD`, and `CHANNEL` in `bruce/.env` or your
shell environment; see [bruce/.env.example](bruce/.env.example).

```bash
cd bruce
cargo run -- game wordle --word-list standard
```

Word lists are `standard`, `challenge`, and `nerd`. Bruce needs Twitch chat
credentials. It does not require an alerts token, an AI key, or a running alerts
server. The `dragons` command remains an unimplemented placeholder.

Bruce defaults to `games.db` in the current directory. Use `--db-path` or
`BRUCE_DB_PATH` to select an existing scoreboard; `--db-ags` remains an alias
for older commands. For example, when running from the workspace root:

```bash
cargo run -p bruce -- game wordle --db-path sqlite:bruce/games.db
```

Both applications load `.env` from the current directory or its ancestors.
Their default logs are `twitch_alerts.log` and `bruce.log`. Override these with
`TWITCH_ALERTS_LOG` or `BRUCE_LOG`; `NULL_TWITCH_LOG` is retained as a fallback.
Existing database files and local configuration are not moved by this split.

Alerts subscribes to follows, subscriptions, resubscriptions, raids, gifted
subscriptions, and (with `bits:read`) cheers, and persists a story for each. The
latest migration preserves existing gift records and permits missing gifter details
for anonymous gifts. If Twitch closes the EventSub connection or asks the server to
move sessions, the listener reconnects and re-creates its subscriptions. Bruce
registers players when they submit valid guesses; round scoring and the visible
leaderboard remain unfinished.

Database and log files such as `alerts.db`, `bruce/games.db`, and `*.log` are not
tracked by Git; copies in older checkouts stay on disk but are no longer committed.

## Verify

From the workspace root:

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

These commands cover both applications, the shared helpers, and the word-list
submodule, including its documentation examples. CI treats compiler and Clippy
warnings as errors.

CI also checks each application separately, so workspace feature unification
cannot hide a missing dependency.

### Test alerts locally without Twitch credentials

Run the executable integration test:

```bash
cargo +1.95 test --locked -p twitch_alerts --test local_auth -- --nocapture
```

It launches local OAuth and EventSub mocks and runs the actual alerts binary with
fake credentials, a temporary SQLite database, and a temporary token file. It
checks authorization and callback state, recovery from an expired token,
background renewal, persistence after a hard restart, EventSub subscriptions,
overlay/admin pages, static assets, and the browser WebSocket connection. It
cleans up its processes and files when it finishes. No real Twitch or OpenAI
requests are made, and it does not load your `.env`.

The alerts test suite also checks that a queued follow alert reaches a browser
WebSocket with the expected HTML:

```bash
cargo +1.95 test --locked -p twitch_alerts
```

These tests need permission to listen on localhost ports. Real Twitch browser
consent and live stream events still need a separate test using the authorization
setup above. Locally generated token files are excluded from Docker's build
context as well as Git.

## Alerts deployment

The root Dockerfile builds only `twitch_alerts` and includes its browser assets.
The obsolete standalone frontend build has been removed. The image requires no
build-time database or runtime migration directory.

```bash
docker build -t twitch-alerts .
```

The default command runs alerts under Litestream using `hack/start.sh` and
`hack/litestream.yaml`. Supply the Twitch, AI, and Litestream credentials at
runtime and persist `/data`. The replica location is configured in those files.
For a local run without Litestream, override the command:

```bash
docker run --rm --env-file .env -p 8080:8080 -p 9000:9000 \
  -v twitch-alerts-data:/data twitch-alerts \
  /usr/local/bin/twitch-alerts server --db-path /data/db
```

`fly.toml` configures the alerts service. The Kubernetes manifests in
`twitch_alerts/deployment` use a separate Litestream sidecar and start the alerts
binary directly. Update their image tag to the image you build before deploying.

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

From the workspace root, configure `ACCESS_TOKEN` (or `OAUTH2_SERVICE_URL`) and
`OPENAI_KEY` in your environment or `.env`. See
[twitch_alerts/.env.example](twitch_alerts/.env.example) for available settings.

```bash
cargo run -p twitch_alerts -- server --channel-login your_channel
```

HTTP defaults to `0.0.0.0:8080`, and the WebSocket listener uses port `9000`.
The overlay is at `/` and the admin UI at `/admin`. `WEBSOCKET_HOST` is the hostname
browsers use to reach the WebSocket listener. `HTTP_PORT` and `WEBSOCKET_PORT`
configure the two ports independently. Browser WebSockets use `ws` on HTTP pages
and `wss` on HTTPS pages.

The database defaults to `alerts.db` in the current directory. Set `DB_PATH` or
`--db-path` to use an existing database. For a copied binary, also set
`FRONTEND_ASSETS` to the copied `twitch_alerts/src/frontend/assets` directory.

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

Alerts persists follow/subscription stories, raids, and gifted subscriptions. The
latest migration preserves existing gift records and permits missing gifter details
for anonymous gifts. Bruce registers players when they submit valid guesses; round
scoring and the visible leaderboard remain unfinished.

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

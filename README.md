# twitch-alerts

https://id.twitch.tv/oauth2/authorize?response_type=token&client_id=3oxst3vtgybw5tfetw7l942bthyzcm&redirect_uri=http://localhost:3000&scope=channel%3Amanage%3Apolls+channel%3Aread%3Apolls+moderator%3Aread%3Afollowers+channel%3Aread%3Asubscriptions&state=c3ab8aa609ea11e793ae92361f002671

## Testing:

To start the websocket server:
```bash
twitch event websocket start-server
```

To trigger an event:
```bash
twitch event trigger channel.ban --transport=websocket
```
twitch alerts:
```bash
RUST_BACKTRACE=full cargo run --bin monolith -- --channel-id="99431252"
```

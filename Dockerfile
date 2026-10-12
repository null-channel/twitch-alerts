FROM rust:1.95-bookworm AS builder

WORKDIR /usr/src/app
COPY . .
RUN cargo build --locked --release -p twitch_alerts
ADD https://github.com/benbjohnson/litestream/releases/download/v0.3.8/litestream-v0.3.8-linux-amd64-static.tar.gz /tmp/litestream.tar.gz
RUN tar -C /usr/local/bin -xzf /tmp/litestream.tar.gz

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data

COPY --from=builder /usr/local/bin/litestream /usr/local/bin/litestream
COPY --from=builder /usr/src/app/target/release/twitch_alerts /usr/local/bin/twitch-alerts
COPY twitch_alerts/src/frontend/assets /var/lib/assets/
COPY --chmod=755 hack/start.sh /scripts/start.sh
COPY hack/litestream.yaml /etc/litestream.yml
ENV FRONTEND_ASSETS=/var/lib/assets
EXPOSE 8080 9000
CMD ["/scripts/start.sh"]

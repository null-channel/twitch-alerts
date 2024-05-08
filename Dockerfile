FROM cgr.dev/chainguard/rust:latest-dev as build
USER root
RUN mkdir -p /usr/src/app
WORKDIR /usr/src/app
COPY . .
RUN cargo build --bin monolith --release

FROM cgr.dev/chainguard/cc-dynamic:latest
COPY ./ai_manager_service/migrations /var/lib/db/migrations
EXPOSE 9000
COPY --from=build --chown=nonroot:nonroot /usr/src/app/target/release/monolith /usr/local/bin/twitch-alerts
CMD ["/usr/local/bin/monolith"]

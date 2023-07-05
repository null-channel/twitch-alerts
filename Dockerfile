FROM rust:1.67 as builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --bin monolith --release

FROM debian:bullseye-slim
#RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/monolith /usr/local/bin/twitch-alerts
CMD ["twitch-alerts"]
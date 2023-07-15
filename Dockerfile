FROM rust:1.67 as builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --bin monolith --release

FROM ubuntu:20.04
RUN apt-get update && apt-get install -y openssl ca-certificates
RUN update-ca-certificates
RUN apt-get install -y libssl-dev
RUN rm -rf /var/lib/apt/lists/*
COPY ./ai_manager_service/migrations /var/lib/db/migrations
EXPOSE 9000
#RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/monolith /usr/local/bin/twitch-alerts
CMD ["twitch-alerts"]
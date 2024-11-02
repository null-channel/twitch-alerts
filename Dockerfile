ARG UBUNTU_RELEASE=22.04

FROM rust:1.79 AS builder

WORKDIR /usr/src/app

COPY . .
RUN cargo build --bin monolith --release
ADD https://github.com/benbjohnson/litestream/releases/download/v0.3.8/litestream-v0.3.8-linux-amd64-static.tar.gz /tmp/litestream.tar.gz
RUN tar -C /usr/local/bin -xzf /tmp/litestream.tar.gz

# RUN chisel cut --release ubuntu-$UBUNTU_RELEASE --root /rootfs \
#    base-files_base \
#    base-files_release-info \
#    ca-certificates_data \
#    libgcc-s1_libs \
#    libc6_libs \
#    libssl3_libs \
#    openssl_bins

FROM ubuntu:22.04
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y ca-certificates

COPY ./ai_manager_service/migrations /var/lib/db/migrations
EXPOSE 9000
# COPY --from=chiselled /rootfs /
COPY --from=builder /usr/local/bin/litestream /usr/local/bin/litestream
COPY --from=builder /usr/src/app/target/release/monolith /usr/local/bin/twitch-alerts
COPY ./frontend_api/assets /var/lib/assets/
COPY scripts/start.sh /scripts/start.sh
COPY scripts/litestream.yaml /etc/litestream.yml
CMD ["/scripts/start.sh"]

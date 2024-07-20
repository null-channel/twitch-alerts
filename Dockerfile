ARG UBUNTU_RELEASE=22.04

FROM rust:1.79 AS builder

WORKDIR /usr/src/app

COPY . .
RUN cargo build --bin monolith --release

# Build the chiselled filesystem based on the desired slices.
FROM ubuntu:$UBUNTU_RELEASE AS chiselled
ARG UBUNTU_RELEASE
ARG TARGETARCH

# Get chisel binary
ADD https://github.com/canonical/chisel/releases/download/v0.9.1/chisel_v0.9.1_linux_$TARGETARCH.tar.gz chisel.tar.gz
RUN tar -xvf chisel.tar.gz -C /usr/bin/
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y ca-certificates

WORKDIR /rootfs

#RUN chisel cut --release ubuntu-$UBUNTU_RELEASE --root /rootfs \
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
COPY --from=builder /usr/src/app/target/release/monolith /usr/local/bin/twitch-alerts
COPY ./frontend_api/assets /var/lib/assets/
CMD ["/usr/local/bin/twitch-alerts"]

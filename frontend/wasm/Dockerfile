# build stage
FROM rust as build-stage
WORKDIR /app
COPY . .
RUN cargo install --locked trunk
WORKDIR /app
RUN trunk build --release --all-features
EXPOSE 8080
CMD ["trunk", "serve", "--release"]
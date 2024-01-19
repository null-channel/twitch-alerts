# build stage
#FROM rust as build-stage
#WORKDIR /app
#COPY . .
#RUN rustup target add wasm32-unknown-unknown
#RUN cargo install --locked trunk
#RUN cargo install --locked wasm-bindgen-cli
#RUN trunk build



# production stage
FROM nginx:stable-alpine as production-stage
COPY nginx.conf /etc/nginx/nginx.conf
COPY dist /usr/share/nginx/html/
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]

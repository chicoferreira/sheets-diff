FROM rust:latest as builder

WORKDIR /app

RUN cargo init

COPY Cargo.toml .
COPY Cargo.lock .

COPY src src

RUN cargo build --release

FROM debian:12-slim as runtime

RUN apt-get update && apt install -y openssl ca-certificates

WORKDIR /app

COPY --from=builder /app/target/release/sheets-diff /app/sheets-diff

COPY client_secret.json /app/client_secret.json
COPY ids.txt /app/ids.txt
COPY token.json /app/token.json

CMD ["./sheets-diff"]
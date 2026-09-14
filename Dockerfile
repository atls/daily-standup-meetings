FROM rust:1.90-bookworm AS build

WORKDIR /usr/src/daily-standup-meetings

COPY Cargo.toml Cargo.lock ./
COPY graphql ./graphql
COPY src ./src

RUN cargo build --release --locked

FROM debian:12-slim

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build \
    /usr/src/daily-standup-meetings/target/release/daily-standup-meetings \
    /usr/local/bin/daily-standup-meetings

ENTRYPOINT ["/usr/local/bin/daily-standup-meetings"]

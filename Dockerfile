FROM rust:slim AS chef
# We only pay the installation cost once,
# it will be cached from the second build onwards
RUN cargo install cargo-chef --locked
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare  --recipe-path recipe.json

FROM docker.io/bufbuild/buf:latest AS buf

FROM chef AS builder
COPY --from=buf /usr/local/bin/buf /usr/local/bin/buf
RUN <<EOF
apt-get update
apt-get install -y protobuf-compiler
rm -rf /var/lib/apt/lists/*
EOF
COPY --from=planner /src/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release --bin rsjudge

FROM debian:stable-slim AS rsjudge
RUN <<EOF
apt-get update
apt-get install -y libcap2-bin
rm -rf /var/lib/apt/lists/*

useradd --create-home \
    --comment "Supervisor of rsjudge" \
    --home /var/lib/rsjudge-supervisor/ \
    --system \
    --shell /sbin/nologin \
    rsjudge-supervisor

useradd --create-home \
    --comment "Builder of rsjudge" \
    --home /var/lib/rsjudge-builder/ \
    --system \
    --shell /sbin/nologin \
    rsjudge-builder

useradd --create-home \
    --comment "Runner of rsjudge" \
    --home /var/lib/rsjudge-runner/ \
    --system \
    --shell /sbin/nologin \
    rsjudge-runner
EOF

COPY --from=builder /src/target/release/rsjudge /app/
RUN setcap "cap_setuid,cap_setgid,cap_dac_read_search=p" /app/rsjudge

USER rsjudge-supervisor
CMD ["/app/rsjudge"]

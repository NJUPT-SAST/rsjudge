FROM rust:1 AS builder

WORKDIR /src

COPY . .

RUN mkdir -p /root/.cargo cat >/root/.cargo/config <<EOF

EOF

RUN cargo build --release

FROM gcr.io/distroless/base-debian12

COPY --from=builder /src/target/release/rsjudge /app

USER rsjudge-supervisor

CMD ["/app/rsjudge"]

# FROM rust:1 as build-env
# WORKDIR /app
# COPY . /app
# RUN cargo build --release

# FROM gcr.io/distroless/cc-debian12
# COPY --from=build-env /app/target/release/hello-world-distroless /
# CMD ["./hello-world-distroless"]

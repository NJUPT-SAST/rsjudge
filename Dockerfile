# Use the official Rust image as the base image
FROM rust:latest AS builder

# Set the working directory inside the container
WORKDIR /app

# Copy the Cargo.toml and Cargo.lock files to the container
COPY Cargo.toml Cargo.lock ./

# Build the dependencies separately to take advantage of Docker layer caching
RUN mkdir -p src && touch src/lib.rs && cargo build --release && rm -rf src

# Copy the source code to the container
COPY build.rs .
COPY src ./src

# Build the project
RUN cargo build --release

# Create a new container for running the build executable
FROM debian:stable-slim

# Create required users
RUN useradd -m -s /sbin/nologin rsjudge-supervisor
RUN useradd -m -s /sbin/nologin rsjudge-builder
RUN useradd -m -s /sbin/nologin rsjudge-runner

# Set the working directory inside the container
WORKDIR /app

# Copy the build executable from the builder container
COPY --from=builder /app/target/release/rsjudge .

COPY templates/* config/

# Set the user
USER rsjudge-supervisor

# Set the entry point for the container
CMD ["./rsjudge", "-c", "config"]

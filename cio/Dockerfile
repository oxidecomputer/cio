# ------------------------------------------------------------------------------
# App Base Stage
# ------------------------------------------------------------------------------
FROM debian:sid-slim AS app-base

RUN apt-get update && apt-get install -y \
	ca-certificates \
	libpq5 \
	libssl1.1 \
	libusb-1.0-0-dev \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

# ------------------------------------------------------------------------------
# Cargo Chef Stage
# ------------------------------------------------------------------------------

FROM rust:latest AS cargo-chef

ENV DEBIAN_FRONTEND=noninteractive

RUN rustup default nightly

RUN cargo install cargo-chef --version 0.1.23

WORKDIR /usr/src/cio

# ------------------------------------------------------------------------------
# Cargo Deps Stage
# ------------------------------------------------------------------------------

FROM cargo-chef AS cargo-deps

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM cargo-chef AS cargo-build

COPY --from=cargo-deps /usr/src/cio/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN cargo build --release --bin cio

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM app-base

COPY --from=cargo-build /usr/src/cio/target/release/cio-api /usr/bin/cio-api

CMD ["cio-api"]

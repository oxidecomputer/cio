# ------------------------------------------------------------------------------
# App Base Stage
# ------------------------------------------------------------------------------
FROM debian:bullseye AS app-base

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
	ca-certificates \
	libpq5 \
	libssl1.1 \
	libusb-1.0-0-dev \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

# ------------------------------------------------------------------------------
# Cargo Nightly Stage
# ------------------------------------------------------------------------------

FROM rust:latest AS cargo-nightly

ENV DEBIAN_FRONTEND=noninteractive

RUN rustup default nightly

WORKDIR /usr/src/cio

# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM cargo-nightly AS cargo-build

RUN apt-get update && apt-get install -y \
	ca-certificates \
	libpq-dev \
	libssl-dev \
	libusb-1.0-0-dev \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

COPY cio/src/dummy.rs ./src/dummy.rs

COPY cio/Cargo.toml ./Cargo.toml

COPY rust-toolchain ./rust-toolchain

COPY macros ../macros

COPY mailchimp-minimal-api ../mailchimp-minimal-api

COPY diesel-sentry ../diesel-sentry

COPY zoho-client ../zoho-client

RUN sed -i 's#main.rs#dummy.rs#' ./Cargo.toml

RUN cargo build --bin cio-api

RUN sed -i 's#dummy.rs#main.rs#' ./Cargo.toml

COPY cio/src src

RUN cargo build --bin cio-api

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM app-base

COPY --from=cargo-build /usr/src/cio/target/debug/cio-api /usr/bin/cio-api

CMD ["cio-api"]

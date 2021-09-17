
# ------------------------------------------------------------------------------
# App Base Stage
# ------------------------------------------------------------------------------
FROM debian:sid-slim AS app-base

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
	ca-certificates \
	libssl1.1 \
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

COPY cfcert/src/dummy.rs ./src/dummy.rs

COPY cfcert/Cargo.toml ./Cargo.toml

RUN sed -i 's#main.rs#dummy.rs#' ./Cargo.toml

RUN cargo build --release --bin cfcert

RUN sed -i 's#dummy.rs#main.rs#' ./Cargo.toml

COPY cfcert/src src

RUN cargo build --release --bin cfcert

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM app-base

COPY --from=cargo-build /usr/src/cio/target/release/cfcert /usr/bin/cfcert

CMD ["cfcert"]

# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM rust:latest as cargo-build

ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/cfcert

RUN rustup default nightly

COPY . .

WORKDIR /usr/src/cfcert/cfcert
RUN cargo build --release

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM debian:sid-slim

RUN apt-get update && apt-get install -y \
	ca-certificates \
	libpq5 \
	libssl1.1 \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

COPY --from=cargo-build /usr/src/cfcert/target/release/cfcert /usr/bin/cfcert

CMD ["cfcert"]

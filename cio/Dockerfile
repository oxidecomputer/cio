# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM rust:nightly as cargo-build

ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/cio-api

COPY . .

WORKDIR /usr/src/cio-api/cio
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


COPY --from=cargo-build /usr/src/cio-api/target/release/cio-api /usr/bin/cio-api

CMD ["cio-api"]

# ------------------------------------------------------------------------------
# App Base Stage
# ------------------------------------------------------------------------------
FROM debian:sid-slim AS app-base

RUN apt-get update && apt-get install -y \
	asciidoctor \
	ca-certificates \
	libpq5 \
	libssl1.1 \
	libusb-1.0-0-dev \
    p7zip \
	pandoc \
	poppler-utils \
	ruby \
    texlive-latex-base \
	texlive-fonts-recommended \
	texlive-fonts-extra \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

RUN gem install \
	asciidoctor-pdf \
	rouge

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

RUN cargo chef prepare --recipe-path recipe.json || true

# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM cargo-chef AS cargo-build

#COPY --from=cargo-deps /usr/src/cio/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json || true

COPY . .

RUN cargo build --release --bin webhooky

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM app-base

COPY --from=cargo-build /usr/src/cio/target/release/webhooky /usr/bin/webhooky

CMD ["webhooky", "server", "--do-cron"]

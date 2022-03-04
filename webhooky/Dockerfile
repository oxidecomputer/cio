# ------------------------------------------------------------------------------
# App Base Stage
# ------------------------------------------------------------------------------
FROM ubuntu:latest AS app-base

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
	asciidoctor \
	ca-certificates \
	libpq5 \
	libssl1.1 \
	libusb-1.0-0-dev \
	lmodern \
    p7zip \
	pandoc \
	poppler-utils \
	ruby \
    texlive-latex-base \
	texlive-fonts-recommended \
	texlive-fonts-extra \
	texlive-latex-extra \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

RUN gem install \
	asciidoctor-pdf \
	rouge

# ------------------------------------------------------------------------------
# Cargo Nightly Stage
# ------------------------------------------------------------------------------

FROM rust:latest AS cargo-nightly

ENV DEBIAN_FRONTEND=noninteractive

RUN rustup default nightly

WORKDIR /usr/src/webhooky

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

COPY webhooky/src/dummy.rs ./src/dummy.rs

COPY webhooky/Cargo.toml ./Cargo.toml

COPY rust-toolchain ./rust-toolchain

# Move the deps we need to compile.
COPY cio ../cio

COPY macros ../macros

COPY diesel-sentry ../diesel-sentry

RUN sed -i 's#main.rs#dummy.rs#' ./Cargo.toml

RUN cargo build --bin webhooky

RUN sed -i 's#dummy.rs#main.rs#' ./Cargo.toml

COPY webhooky/src ./src

RUN cargo build --bin webhooky

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM app-base

COPY --from=cargo-build /usr/src/webhooky/target/debug/webhooky /usr/bin/webhooky

CMD ["webhooky", "--json", "server", "--do-cron"]

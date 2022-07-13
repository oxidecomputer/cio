# ------------------------------------------------------------------------------
# App Base Stage
# ------------------------------------------------------------------------------
FROM debian:bullseye AS app-base

ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/webhooky

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
	curl \
    texlive-latex-base \
	texlive-fonts-recommended \
	texlive-fonts-extra \
	texlive-latex-extra \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_16.x | bash - && \
	apt install -y --no-install-recommends \
	nodejs

RUN gem install \
	asciidoctor-pdf \
	asciidoctor-mermaid \
	rouge

RUN cd /usr/local/lib && \
	npm install @mermaid-js/mermaid-cli && \
	ln -s ../lib/node_modules/.bin/mmdc /usr/local/bin/mmdc

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

COPY Cargo.lock ./Cargo.lock

COPY rust-toolchain ./rust-toolchain

# Move the deps we need to compile.
COPY cio ../cio

COPY macros ../macros

COPY mailchimp-minimal-api ../mailchimp-minimal-api

COPY diesel-sentry ../diesel-sentry

COPY zoho-client ../zoho-client

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

CMD ["webhooky", "--json", "server"]

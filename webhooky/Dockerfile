# ------------------------------------------------------------------------------
# App Base Stage
# ------------------------------------------------------------------------------
FROM debian:bullseye AS app-base

RUN groupadd -g 1023 webhooky && \
    useradd -r -u 1023 -g webhooky webhooky

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
	ruby-dev \
	make \
	curl \
  texlive-latex-base \
	texlive-fonts-recommended \
	texlive-fonts-extra \
	texlive-latex-extra \
	# Additional deps for Pupeteer
	libatk-bridge2.0-0 \
	libgtk-3.0 \
	libasound2 \
	libgbm-dev \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_16.x | bash - && \
	apt-get install -y --no-install-recommends \
	nodejs

RUN gem install rouge

RUN gem install asciidoctor-pdf -v 2.3.15
RUN gem install asciidoctor-mermaid -v 0.4.1

# Create the cache directory that pupeteer expected to exist
RUN mkdir -p /home/webhooky/.cache/puppeteer

RUN cd /usr/local/lib && \
	PUPPETEER_CACHE_DIR=/home/webhooky/.cache/puppeteer npm install @mermaid-js/mermaid-cli && \
	ln -s ../lib/node_modules/.bin/mmdc /usr/local/bin/mmdc

USER webhooky

# ------------------------------------------------------------------------------
# Cargo Stage
# ------------------------------------------------------------------------------

FROM rust:bullseye AS cargo

ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/webhooky


# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM cargo AS cargo-build

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

COPY rust-toolchain.toml ./rust-toolchain.toml

COPY airtable ../airtable

COPY checkr ../checkr

# Move the deps we need to compile.
COPY cio ../cio

COPY cio-api-types ../cio-api-types

COPY docusign ../docusign

COPY google-geocode ../google-geocode

COPY macros ../macros

COPY mailerlite ../mailerlite

COPY mailchimp-minimal-api ../mailchimp-minimal-api

COPY meilisearch-minimal-api ../meilisearch-minimal-api

COPY quickbooks ../quickbooks

COPY partial-struct ../partial-struct

COPY parse-rfd ../parse-rfd

COPY ramp-minimal-api ../ramp-minimal-api

COPY shippo ../shippo

COPY slack ../slack

COPY tailscale ../tailscale

COPY dropshot-verify-request ../dropshot-verify-request

COPY zoho-client ../zoho-client

RUN sed -i 's#main.rs#dummy.rs#' ./Cargo.toml

RUN cargo build --bin webhooky --release

RUN sed -i 's#dummy.rs#main.rs#' ./Cargo.toml

COPY webhooky/src ./src

RUN cargo build --bin webhooky --release

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM app-base

COPY --from=cargo-build /usr/src/webhooky/target/release/webhooky /usr/bin/webhooky

CMD ["webhooky", "--json", "server"]


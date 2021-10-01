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

WORKDIR /usr/src/cio

# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM cargo-nightly AS cargo-build

COPY webhooky/src/dummy.rs ./src/dummy.rs

COPY webhooky/Cargo.toml ./Cargo.toml

RUN sed -i 's#main.rs#dummy.rs#' ./Cargo.toml

RUN cargo build --release --bin webhooky

RUN sed -i 's#dummy.rs#main.rs#' ./Cargo.toml

COPY webhooky/src src

RUN cargo build --release --bin webhooky

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM app-base

COPY --from=cargo-build /usr/src/cio/target/release/webhooky /usr/bin/webhooky

CMD ["webhooky", "server", "--do-cron"]

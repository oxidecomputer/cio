# ------------------------------------------------------------------------------
# Cargo Build Stage
# ------------------------------------------------------------------------------

FROM rust:latest as cargo-build

ENV DEBIAN_FRONTEND=noninteractive

WORKDIR /usr/src/webhooky

RUN rustup default nightly

COPY . .

WORKDIR /usr/src/webhooky/webhooky
RUN cargo build --release

# ------------------------------------------------------------------------------
# Final Stage
# ------------------------------------------------------------------------------

FROM debian:sid-slim

RUN apt-get update && apt-get install -y \
	asciidoctor \
	ca-certificates \
	libpq5 \
	libssl1.1 \
	pandoc \
	poppler-utils \
	ruby \
	--no-install-recommends \
	&& rm -rf /var/lib/apt/lists/*

RUN gem --no-rdoc --no-ri install \
	asciidoctor-pdf \
	rouge

COPY --from=cargo-build /usr/src/webhooky/target/release/webhooky /usr/bin/webhooky

CMD ["webhooky"]

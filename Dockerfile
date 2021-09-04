FROM rust:1.54-alpine as builder
WORKDIR /usr/src/proxy
RUN apk add --no-cache musl-dev libressl-dev
COPY . .
RUN RUSTFLAGS="" cargo build --release

FROM alpine:latest
MAINTAINER suika <suikalala@gmail.com>

ENV LISTEN=""
ENV MODE=""
ENV PROXY=""
ENV PRIVATE_KEY=""
ENV FULLCHAIN=""

COPY ./entrypoint.sh /
RUN chmod +x /entrypoint.sh && apk add --no-cache ca-certificates
COPY --from=builder /usr/src/proxy/target/release/ss /usr/local/bin/ss
ENTRYPOINT ["/entrypoint.sh"]

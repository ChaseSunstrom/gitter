# Gitter: one small image with the server, the web app and git.
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
# Build dependencies first so code changes don't rebuild them.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && touch src/lib.rs \
 && cargo build --release --locked && rm -rf src
COPY src src
COPY static static
RUN touch src/main.rs src/lib.rs && cargo build --release --locked

FROM alpine:3.22
# ssh-keygen checks SSH-signed commits. gpg (for GPG-signed ones) would add about 16 MB,
# so it's left out: add `gpg` here to check those too; without it they show "Can't verify".
RUN apk add --no-cache git git-daemon tini openssh-keygen \
 && adduser -D -H -h /data -u 1000 gitter \
 && mkdir /data && chown gitter:gitter /data
COPY --from=build /src/target/release/gitter /usr/local/bin/gitter
USER gitter
ENV GITTER_DATA=/data GITTER_ADDR=0.0.0.0:3000
VOLUME /data
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD wget -qO- http://127.0.0.1:3000/healthz || exit 1
ENTRYPOINT ["/sbin/tini", "--", "gitter"]

FROM rust:1.85-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY assets ./assets
RUN cargo build --release --locked --features server --bins -j 2

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git ffmpeg tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home gitarium \
    && mkdir -p /var/cache/gitarium && chown gitarium:gitarium /var/cache/gitarium
COPY --from=build /app/target/release/gitarium /usr/local/bin/gitarium
COPY --from=build /app/target/release/gitarium-server /usr/local/bin/gitarium-server
ENV GITARIUM_LISTEN=0.0.0.0:8080 GITARIUM_CACHE_DIR=/var/cache/gitarium
USER gitarium
EXPOSE 8080
VOLUME /var/cache/gitarium
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 CMD ["gitarium-server", "--healthcheck"]
ENTRYPOINT ["/usr/bin/tini", "-g", "--"]
CMD ["gitarium-server"]

# syntax=docker/dockerfile:1.7

# Builds the yapspace binary with cargo-chef, so dependency layers are cached between builds.
FROM rust:1.98-slim-trixie AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG GIT_SHA=unknown
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true \
    GIT_SHA=${GIT_SHA}
RUN cargo build --release --locked -p yapspace-server --bin yapspace

# Minimal runtime: no shell, no package manager, runs as the nonroot user (uid 65532).
FROM gcr.io/distroless/cc-debian13:nonroot
ARG GIT_SHA=unknown
LABEL org.opencontainers.image.title="yapspace" \
      org.opencontainers.image.revision="${GIT_SHA}" \
      org.opencontainers.image.source="https://github.com/OWNER/yapspace"
COPY --from=builder /app/target/release/yapspace /app/yapspace
USER nonroot:nonroot
EXPOSE 8080 9090
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=3 \
  CMD ["/app/yapspace", "healthcheck"]
ENTRYPOINT ["/app/yapspace"]
CMD ["serve"]

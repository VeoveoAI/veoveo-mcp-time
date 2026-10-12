# syntax=docker/dockerfile:1.27@sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e
# Stages: dev is the OpenShell environment, check is the single gate, build compiles the
# release binary, and runtime is the deployable image.

ARG COMPONENT=veoveo-mcp-time

FROM rust:1.99.0-trixie@sha256:6ff07edce8775d0f64be7aba9197229407301bddf2054d62c27b541a6238a181 AS dev
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates clang cmake git pkg-config procps \
    && rm -rf /var/lib/apt/lists/* /var/log/apt /var/log/dpkg.log \
    && rustup component add clippy rustfmt
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    cargo install --locked \
        cargo-nextest@0.9.148 \
        cargo-deny@0.20.2 \
        cargo-hack@0.6.45 \
        cargo-semver-checks@0.51.0 \
        cargo-insta@1.49.0
# Native Store tests run one SurrealDB server process per fixture; no container runtime enters the sandbox.
COPY --from=surrealdb/surrealdb:v3.3.0@sha256:681c6c22c287421b5c7d99e0fde79b6e0d32c36c1ddeaab2762a1661cb04cd20 /surreal /usr/local/bin/surreal
COPY env/profile.d/rust.sh /etc/profile.d/rust.sh
# A development environment installs toolchains, components and crates, so the sandbox user owns them.
RUN useradd --create-home --uid 1000 --shell /bin/bash sandbox \
    && chown -R sandbox:sandbox /usr/local/rustup /usr/local/cargo
USER sandbox

FROM dev AS source
WORKDIR /home/sandbox/src
COPY --chown=sandbox:sandbox . .

FROM source AS check
RUN cargo fmt --check \
    && cargo clippy --locked --all-targets --all-features -- -D warnings \
    && cargo deny --locked check \
    && cargo hack check --each-feature --no-dev-deps \
    && cargo nextest run --locked --profile ci

FROM source AS build
ARG COMPONENT
RUN cargo build --locked --release --bin "${COMPONENT}" \
    && cp "target/release/${COMPONENT}" /home/sandbox/component

FROM debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f AS runtime
ARG COMPONENT
# Time compiles IANA zone releases with zic and reads the leap-second table at runtime.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/* /var/log/apt /var/log/dpkg.log \
    && test -x /usr/sbin/zic \
    && test -f /usr/share/zoneinfo/tzdata.zi \
    && test -f /usr/share/zoneinfo/leap-seconds.list \
    && useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo component \
    && mkdir -p /var/lib/veoveo/time/acquisitions /var/lib/veoveo/time/releases /var/lib/veoveo/time/keys \
    && chown -R component:component /var/lib/veoveo/time
COPY --from=build /home/sandbox/component /usr/local/bin/${COMPONENT}
USER component
EXPOSE 8800
ENTRYPOINT ["/usr/local/bin/veoveo-mcp-time"]
LABEL org.opencontainers.image.source="https://github.com/VeoveoAI/veoveo-mcp-time" \
      org.opencontainers.image.licenses="Apache-2.0"

# Server image (RF-C1, ADR-0006 D5 and D8). Build from the repo root:
#   docker buildx build --platform linux/amd64 -f apps/server/Dockerfile \
#     --build-arg APP_VERSION=sha-0000000 -t ghcr.io/kike1303/torneo-absurdo/server:local --load .
#
# Every build stage runs on $BUILDPLATFORM and cross-compiles to x86_64-unknown-linux-musl with
# cargo-zigbuild, so an ARM Mac builds without emulation. The final stage has no RUN.
# The rust: version must equal the channel of rust-toolchain.toml (scripts/check-compose.mjs).

# rust:1.99.1-slim-bookworm, multi-arch index digest (linux/amd64 and linux/arm64).
FROM --platform=$BUILDPLATFORM rust:1.99.1-slim-bookworm@sha256:11d39e118fbc6aba0c89511ff6810dcbad03e7c28d78e3f9d59b714f8d4e08dc AS chef
ARG BUILDARCH
WORKDIR /app

# wget downloads zig and xz-utils unpacks it (the slim image has neither); gcc (host build
# scripts) and ca-certificates already come with it.
RUN apt-get update \
    && apt-get install -y --no-install-recommends wget xz-utils \
    && rm -rf /var/lib/apt/lists/*

# zig 0.15.2 (cargo-zigbuild's C compiler and linker), one checksum per builder architecture.
RUN set -eu; \
    case "$BUILDARCH" in \
      amd64) arch=x86_64; sum=02aa270f183da276e5b5920b1dac44a63f1a49e55050ebde3aecc9eb82f93239 ;; \
      arm64) arch=aarch64; sum=958ed7d1e00d0ea76590d27666efbf7a932281b3d7ba0c6b01b0ff26498f667f ;; \
      *) echo "no zig checksum for $BUILDARCH" >&2; exit 1 ;; \
    esac; \
    wget --https-only -q -O /tmp/zig.tar.xz "https://ziglang.org/download/0.15.2/zig-$arch-linux-0.15.2.tar.xz"; \
    echo "$sum  /tmp/zig.tar.xz" | sha256sum -c -; \
    mkdir /opt/zig; \
    tar -xJf /tmp/zig.tar.xz -C /opt/zig --strip-components=1; \
    rm /tmp/zig.tar.xz
ENV PATH=/opt/zig:$PATH

# Same toolchain as the image: rustup adds the targets of rust-toolchain.toml (musl and, for the
# engine checks, wasm32-unknown-unknown and wasm32-wasip1, unused here) and the listed components.
COPY rust-toolchain.toml ./
RUN rustup toolchain install --no-self-update

RUN cargo install --locked --version 0.1.78 cargo-chef \
    && cargo install --locked --version 0.23.4 cargo-zigbuild \
    && rm -rf "$CARGO_HOME/registry" "$CARGO_HOME/git"

# sqlx macros never reach a database at build time (migrate! only reads apps/server/migrations).
ENV SQLX_OFFLINE=true

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY apps/server apps/server
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cook
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --zigbuild --release --locked --target x86_64-unknown-linux-musl -p server \
    --recipe-path recipe.json

FROM cook AS build
# After the cook, so a new version does not invalidate the dependency layer. An empty build arg
# would reach build.rs as APP_VERSION="" and be rejected, hence the default.
ARG APP_VERSION=dev
COPY Cargo.toml Cargo.lock ./
COPY apps/server apps/server
COPY crates crates
# The alias word lists and the reserved names, embedded with include_str! (spec 008 RF-B7).
COPY content/names content/names
RUN cargo zigbuild --release --locked --target x86_64-unknown-linux-musl -p server

# distroless/static-debian13:nonroot, index digest (uid 65532, no shell).
FROM gcr.io/distroless/static-debian13:nonroot@sha256:516757106de35205efb70b91ba03f6f1298424a0678c603255cf2852a7000087
COPY --from=build /app/target/x86_64-unknown-linux-musl/release/server /server
# The characters of the new fights (010 RNF "Imagen"). Public in the repo; their license does not change.
COPY content/characters /content/characters
ENV CONTENT_DIR=/content/characters
EXPOSE 8080
# No CMD: without arguments the binary runs `serve`.
ENTRYPOINT ["/server"]

# Web image (RF-C2, ADR-0006 D5). Build from the repo root:
#   docker buildx build --platform linux/amd64 -f apps/web/Dockerfile \
#     -t ghcr.io/kike1303/torneo-absurdo/web:local --load .
#
# The adapter-node output is plain JS, so the build stage runs on $BUILDPLATFORM without
# emulation. Every dependency of apps/web is a devDependency bundled by Vite: the final image has
# build/ and package.json only, without node_modules. The node: version must equal .node-version
# and the final base must be nodejs<major>- (scripts/check-compose.mjs).

# node:22.23.4-bookworm-slim, multi-arch index digest (linux/amd64 and linux/arm64).
FROM --platform=$BUILDPLATFORM node:22.23.4-bookworm-slim@sha256:4b150b48b4116e2a22f2c39cc27025fbf4f153e2078fd3b14d159fdb5c7279da AS build
# corepack fetches the pnpm of `packageManager` and verifies it against its hash.
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0
RUN corepack enable
WORKDIR /app

# Manifests first, so a source change does not invalidate the install layer.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml .npmrc ./
COPY apps/web/package.json apps/web/package.json
COPY packages/protocol-ts/package.json packages/protocol-ts/package.json
COPY packages/renderer/package.json packages/renderer/package.json
RUN --mount=type=cache,id=torneo-pnpm-store,target=/pnpm/store \
    pnpm install --frozen-lockfile --store-dir /pnpm/store --filter 'web...'

COPY apps/web apps/web
COPY packages packages
# Only for the name and the epithet of each fighter (010 RF56): nothing else of content/ is read,
# and the final stage does not have it.
COPY content/characters content/characters
RUN pnpm --filter web build

# distroless/nodejs22-debian13:nonroot, index digest (Node v22.23.4, uid 65532, no shell).
FROM gcr.io/distroless/nodejs22-debian13:nonroot@sha256:8ad9cc935b7843c4024b7e1487b5ef472c354ea847b97c667023e2618552607e
WORKDIR /app
COPY --from=build /app/apps/web/build build/
# "type": "module" for build/index.js.
COPY --from=build /app/apps/web/package.json package.json
ENV NODE_ENV=production
EXPOSE 3000
# The entrypoint is /nodejs/bin/node.
CMD ["build/index.js"]

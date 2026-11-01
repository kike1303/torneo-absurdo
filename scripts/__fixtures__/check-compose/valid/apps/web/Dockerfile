# Web image (RF-C2, ADR-0006 D5). Build from the repo root:
#   docker buildx build --platform linux/amd64 -f apps/web/Dockerfile \
#     -t ghcr.io/kike1303/torneo-absurdo/web:local --load .
#
# The adapter-node output is plain JS, so the build stage runs on $BUILDPLATFORM without
# emulation. Every dependency of apps/web is a devDependency bundled by Vite: the final image has
# build/ and package.json only, without node_modules. The node: version must equal .node-version
# and the final base must be nodejs<major>- (scripts/check-compose.mjs).

# node:22.23.4-bookworm-slim, multi-arch index digest (linux/amd64 and linux/arm64).
FROM --platform=$BUILDPLATFORM node:22.23.4-bookworm-slim@sha256:de170b39439eb0e5a3444ad7d60b015614a381e10fecae2e30ec9e735122f38d AS build
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
FROM gcr.io/distroless/nodejs22-debian13:nonroot@sha256:b0b1352373f14fe86376d423e6d283ea6b85b912ff55ca89bed8d2fe11c257b8
WORKDIR /app
COPY --from=build /app/apps/web/build build/
# "type": "module" for build/index.js.
COPY --from=build /app/apps/web/package.json package.json
ENV NODE_ENV=production
EXPOSE 3000
# The entrypoint is /nodejs/bin/node.
CMD ["build/index.js"]

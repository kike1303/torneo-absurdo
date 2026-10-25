#!/usr/bin/env bash
# RF-C3 and plan-c decision 11: proves with Docker that .dockerignore keeps secrets and heavy
# directories out of the build context.
#
# Never touches the developer's files: a subject that already exists (the real .env of
# `just dev`) is used as is, without opening it. A missing subject is created with a canary under
# noclobber (`set -C`) and only the files created here are removed on exit. On failure only the
# path that leaked is printed, never its contents.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

subjects="infra/.env apps/server/.env apps/web/.env"
created=""
tmp=""

cleanup() {
    for file in $created; do
        rm -f "$file"
    done
    if [ -n "$tmp" ]; then
        rm -rf "${tmp:?}"
    fi
}
trap cleanup EXIT

for file in $subjects; do
    if [ ! -e "$file" ]; then
        # noclobber: if the file appears between the test and the write, this fails instead of
        # overwriting it.
        (set -C; printf 'CHECK_DOCKERIGNORE_CANARY=%s\n' "$$-$file" >"$file")
        created="$created $file"
    fi
done

tmp="$(mktemp -d)"

docker buildx build --quiet -f scripts/fixtures/context.Dockerfile \
    --output "type=local,dest=$tmp" . >/dev/null

ctx="$tmp/ctx"
if [ ! -d "$ctx" ]; then
    echo "check-dockerignore: the exported context is empty" >&2
    exit 1
fi

failed=0
for path in infra/.env apps/server/.env apps/web/.env private .git content/_local; do
    if [ -e "$ctx/$path" ] || [ -L "$ctx/$path" ]; then
        echo "leaked into build context: $path" >&2
        failed=1
    fi
done

# Any node_modules or target directory, at any depth.
leaks="$(cd "$ctx" && find . -type d \( -name node_modules -o -name target \) -prune -print)"
if [ -n "$leaks" ]; then
    printf '%s\n' "$leaks" | sed 's|^\./|leaked into build context: |' >&2
    failed=1
fi

if [ ! -f "$ctx/apps/server/.env.example" ]; then
    echo "missing from build context: apps/server/.env.example (the !**/.env.example rule)" >&2
    failed=1
fi

if [ "$failed" -ne 0 ]; then
    exit 1
fi
echo "check-dockerignore: ok"

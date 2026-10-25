#!/usr/bin/env bash
# RF-C17 and plan-c decision 18 (job `policy`): in a temporary clone, `just hooks` sets
# core.hooksPath to scripts/git-hooks, and the pre-push hook is executable (100755 in the index
# and on disk), POSIX sh and runs `just check`. Never touches the git config of this checkout.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp:?}"' EXIT

git clone -q "$root" "$tmp/repo" 2>/dev/null
cd "$tmp/repo"

just hooks

path="$(git config --local --get core.hooksPath || true)"
if [ "$path" != scripts/git-hooks ]; then
    echo "just hooks left core.hooksPath='$path', expected scripts/git-hooks" >&2
    exit 1
fi

hook=scripts/git-hooks/pre-push
mode="$(git ls-files -s "$hook" | cut -d' ' -f1)"
if [ "$mode" != 100755 ]; then
    echo "$hook has mode '$mode' in the index, expected 100755" >&2
    exit 1
fi
if [ ! -x "$hook" ]; then
    echo "$hook is not executable in the clone" >&2
    exit 1
fi
if [ "$(head -n1 "$hook")" != '#!/bin/sh' ]; then
    echo "$hook must start with #!/bin/sh" >&2
    exit 1
fi
sh -n "$hook"
if ! grep -Eq '^exec just check$' "$hook"; then
    echo "$hook must run \`exec just check\`" >&2
    exit 1
fi
echo "check-hooks: ok"

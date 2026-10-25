#!/usr/bin/env bash
# RF-C7 and plan-c decision 12: the external networks of infra/docker-compose.yml, for
# `just prod-local` only. Same parameters as on the VPS (ADR-0006 D1), plus the label
# torneo.local=1 so `down` removes only what this script created.
#
#   scripts/local-networks.sh up     create the missing networks (idempotent)
#   scripts/local-networks.sh down   remove only the networks labelled torneo.local=1
#
# A network that already exists without the label (for example a real torneo-edge) is used as is
# and never removed.
set -euo pipefail

LABEL=torneo.local=1

ensure() {
    local name=$1
    shift
    if docker network inspect "$name" >/dev/null 2>&1; then
        local label
        label="$(docker network inspect -f '{{index .Labels "torneo.local"}}' "$name")"
        if [ "$label" != 1 ]; then
            echo "warning: network $name exists without the $LABEL label: using it unchanged" >&2
        fi
        return 0
    fi
    local err
    if ! err="$(docker network create --label "$LABEL" "$@" "$name" 2>&1 >/dev/null)"; then
        echo "could not create network $name: $err" >&2
        echo "existing networks and their subnets:" >&2
        docker network ls -q | xargs docker network inspect \
            -f '  {{.Name}} {{range .IPAM.Config}}{{.Subnet}} {{end}}' >&2
        echo "If one of them overlaps $name, remove it (docker network rm <name>) or free the" >&2
        echo "subnet, then run \`just prod-local\` again." >&2
        exit 1
    fi
    echo "created network $name"
}

case "${1:-}" in
    up)
        ensure torneo-edge --driver bridge --subnet 10.250.0.0/28 --gateway 10.250.0.1 \
            --ip-range 10.250.0.8/29
        ensure torneo-db --driver bridge
        ;;
    down)
        ids="$(docker network ls -q --filter "label=$LABEL")"
        if [ -n "$ids" ]; then
            # shellcheck disable=SC2086 # one id per word
            docker network rm $ids >/dev/null
            echo "removed the networks labelled $LABEL"
        fi
        ;;
    *)
        echo "usage: $0 up|down" >&2
        exit 2
        ;;
esac

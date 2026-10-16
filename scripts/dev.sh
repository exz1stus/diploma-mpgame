#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s [-n client_count]\n' "$0"
}

client_count=1
while getopts ':n:h' option; do
    case "$option" in
        n) client_count=$OPTARG ;;
        h) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done
shift $((OPTIND - 1))

if (( $# != 0 )) || [[ ! $client_count =~ ^[0-9]+$ ]]; then
    usage >&2
    exit 2
fi

cd "$(dirname "$0")/.."

pids=()
cleanup() {
    trap - EXIT INT TERM
    for pid in "${pids[@]}"; do
        kill -- "-$pid" 2>/dev/null || true
    done
    wait 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

start_process() {
    local package=$1 label=$2
    setsid bash -c 'set -o pipefail; cargo run -p "$1" 2>&1 | sed -u "s/^/[$2] /"' \
        _ "$package" "$label" &
    pids+=("$!")
}

start_process server server
server_pid=${pids[0]}

for ((i = 1; i <= client_count; i++)); do
    start_process client "client $i"
done

wait "$server_pid"

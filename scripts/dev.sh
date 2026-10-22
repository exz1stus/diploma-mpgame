#!/usr/bin/env bash
set -euo pipefail

usage() {
    printf 'Usage: %s [-n client_count] [-s empty|circle]\n' "$0"
}

client_count=1
scene=empty
while getopts ':n:s:h' option; do
    case "$option" in
        n) client_count=$OPTARG ;;
        s) scene=$OPTARG ;;
        h) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done
shift $((OPTIND - 1))

if (( $# != 0 )) || [[ ! $client_count =~ ^[0-9]+$ ]] || [[ $scene != empty && $scene != circle ]]; then
    usage >&2
    exit 2
fi

cd "$(dirname "$0")/.."

cargo build -p server -p client
binary_dir="${CARGO_TARGET_DIR:-target}/debug"

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
    local -a command=("$binary_dir/$package")
    if [[ $package == client ]]; then
        command=(env "TEST_SCENE=$scene" "${command[@]}")
    fi
    setsid bash -c 'set -o pipefail; label=$1; shift; "$@" 2>&1 | sed -u "s/^/[$label] /"' \
        _ "$label" "${command[@]}" &
    pids+=("$!")
}

start_process server server
server_pid=${pids[0]}

for ((i = 1; i <= client_count; i++)); do
    start_process client "client $i"
done

wait "$server_pid"

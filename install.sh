#!/usr/bin/env bash
set -euo pipefail

destination_dir="${DESTDIR:-}/usr/bin"
if [[ $destination_dir != /* ]]; then
    destination_dir="$PWD/$destination_dir"
fi
destination="$destination_dir/archoxide"

project_dir=$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
cd -- "$project_dir"

target_dir="$project_dir/target"
host=$(rustc --print host-tuple)
cargo build --locked --release --bin archoxide --target "$host" --target-dir "$target_dir"
binary="$target_dir/$host/release/archoxide"

admin=()
if [[ -n ${DESTDIR:-} ]]; then
    mkdir -p -- "$destination_dir"
elif (( EUID != 0 )); then
    admin=(sudo --)
fi

action=Installed
if [[ -e $destination || -L $destination ]]; then
    action=Updated
fi

# Stage the new executable beside its destination so the rename is atomic.
staged=$("${admin[@]}" mktemp -- "$destination.XXXXXXXXXX")
trap '"${admin[@]}" rm -f -- "$staged"' EXIT
"${admin[@]}" install -m 0755 -- "$binary" "$staged"
"${admin[@]}" mv -fT -- "$staged" "$destination"
trap - EXIT
printf '%s %s\n' "$action" "$destination"

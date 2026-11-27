#!/usr/bin/env bash
set -euo pipefail

source_package=$(realpath "$1")
output_dir=$(realpath -m "$2")
manifest=$(realpath "$3")
release=$4
template=$(dirname "$(realpath "$0")")/PKGBUILD.in
version=$(cargo metadata --manifest-path "$manifest" --locked --no-deps --format-version 1 |
    jq -er --arg manifest "$manifest" '.packages[] | select(.manifest_path == $manifest) | .version')
version=${version//-/_}
version=${version//+/_}
if [[ ! "$version" =~ ^[[:alnum:]._]+$ || ! "$release" =~ ^[1-9][0-9]*$ ]]; then
    echo 'Invalid Arch package version or release number.' >&2
    exit 1
fi

work_dir=$(mktemp -d)
trap 'rm -rf -- "$work_dir"' EXIT
mkdir -p "$output_dir/release" "$output_dir/aur" "$work_dir/payload" "$work_dir/recipe"
bsdtar -xOf "$source_package" usr/bin/archoxide > "$work_dir/payload/archoxide"
bsdtar -xOf "$source_package" usr/share/licenses/archoxide-git/LICENSE > "$work_dir/payload/LICENSE"
chmod 755 "$work_dir/payload/archoxide"

asset="archoxide-$version-$release-x86_64.tar.gz"
tar --mtime=@0 --owner=0 --group=0 --numeric-owner -C "$work_dir/payload" \
    -czf "$output_dir/release/$asset" archoxide LICENSE
digest=$(sha256sum "$output_dir/release/$asset")
sed -e "s/@VERSION@/$version/g" -e "s/@RELEASE@/$release/g" -e "s/@SHA256@/${digest%% *}/g" \
    "$template" > "$work_dir/recipe/PKGBUILD"

cd "$work_dir/recipe"
cp "$output_dir/release/$asset" .
makepkg --printsrcinfo > .SRCINFO
makepkg --noconfirm
package="archoxide-bin-$version-$release-x86_64.pkg.tar.zst"
bsdtar -xOf "$package" usr/bin/archoxide > "$work_dir/archoxide-check"
chmod 755 "$work_dir/archoxide-check"
"$work_dir/archoxide-check" --help
install -m 644 "$package" "$output_dir/release/"
install -m 644 PKGBUILD .SRCINFO "$work_dir/payload/LICENSE" "$output_dir/aur/"

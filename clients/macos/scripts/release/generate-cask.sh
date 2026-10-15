#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 || $1 != *.dmg ]]; then
    echo "usage: $0 <disk-image.dmg> <cask.rb>" >&2
    exit 64
fi
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$ROOT/../apple/scripts/apple-platform.sh"
kodosi_apple_platform macosx
dmg=$1
output=$2
: "${KODOSI_DOWNLOAD_URL:?Set KODOSI_DOWNLOAD_URL to the public HTTPS DMG URL}"
if [[ ! -f "$dmg" ]]; then
    echo "$dmg does not exist. Make the disk image with package-dmg.sh." >&2
    exit 1
fi
scratch=$(mktemp -d "${TMPDIR:-/tmp}/kodosi-cask.XXXXXX")
trap 'hdiutil detach -quiet "$scratch/volume" 2>/dev/null || true; rm -rf "$scratch"' EXIT
curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --output "$scratch/public.dmg" "$KODOSI_DOWNLOAD_URL"
sha256=$(shasum -a 256 "$dmg" | cut -d' ' -f1)
if [[ "$(shasum -a 256 "$scratch/public.dmg" | cut -d' ' -f1)" != "$sha256" ]]; then
    echo "The disk image at $KODOSI_DOWNLOAD_URL differs from $dmg." >&2
    exit 1
fi
hdiutil attach -quiet -readonly -nobrowse -noautoopen -mountpoint "$scratch/volume" "$dmg"
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
    "$scratch/volume/KodosiDesktop.app/Contents/Info.plist")
hdiutil detach -quiet "$scratch/volume"
mkdir -p "$(dirname "$output")"
# No zap stanza: an uninstall keeps the Kodosi data and the device identity of this Mac.
cat >"$output" <<RUBY
cask "kodosi-desktop" do
  version "$version"
  sha256 "$sha256"

  url "$KODOSI_DOWNLOAD_URL"
  name "Kodosi Desktop"
  desc "Local terminals with trusted device and friend access"
  homepage "https://github.com/johnkozaris/Kodosi"

  depends_on arch: :arm64
  depends_on macos: ">= $KODOSI_DEPLOYMENT_TARGET"

  app "KodosiDesktop.app"
  binary "KodosiDesktop.app/Contents/MacOS/KodosiDesktop", target: "kodosi"

  uninstall quit: "com.kodosi.desktop"
end
RUBY
ruby -c "$output"
echo "$output"

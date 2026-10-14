#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 || $1 != *.dmg ]]; then
    echo "usage: $0 <disk-image.dmg> <cask.rb>" >&2
    exit 64
fi
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
    "$scratch/volume/Kodosi.app/Contents/Info.plist")
hdiutil detach -quiet "$scratch/volume"
mkdir -p "$(dirname "$output")"
# The cask has no zap stanza, so an uninstall keeps the device enrollment in ~/.kodosi.
cat >"$output" <<RUBY
cask "kodosi" do
  version "$version"
  sha256 "$sha256"

  url "$KODOSI_DOWNLOAD_URL"
  name "Kodosi"
  desc "Real terminals on your machines, shared with trusted people"
  homepage "https://kodosi.com"

  livecheck do
    url "https://updates.kodosi.com/macos/appcast.xml"
    strategy :sparkle
  end

  auto_updates true
  depends_on arch: :arm64
  depends_on macos: ">= :tahoe"

  app "Kodosi.app"
  binary "#{appdir}/Kodosi.app/Contents/MacOS/Kodosi", target: "kodosi"

  uninstall quit: "com.kodosi.desktop"
end
RUBY
ruby -c "$output"
echo "$output"

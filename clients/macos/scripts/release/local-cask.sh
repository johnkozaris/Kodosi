#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

source "$ROOT/scripts/build/deployment-target.sh"
kodosi_load_macos_deployment_target "$ROOT"
cd "$ROOT"
: "${KODOSI_DOWNLOAD_URL:?Set KODOSI_DOWNLOAD_URL to the public HTTPS DMG URL}"
case "$KODOSI_DOWNLOAD_URL" in
    https://*) ;;
    *) echo "KODOSI_DOWNLOAD_URL must use HTTPS" >&2; exit 1 ;;
esac
dmg="build/export/KodosiDesktop.dmg"
app="build/export/KodosiDesktop.app"
downloaded_dmg="build/public/KodosiDesktop.dmg"
output="build/KodosiDesktop.rb"
test -f "$dmg"
test -d "$app"
mkdir -p "$(dirname "$downloaded_dmg")"
curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --output "$downloaded_dmg" "$KODOSI_DOWNLOAD_URL"
local_sha256=$(shasum -a 256 "$dmg" | cut -d' ' -f1)
sha256=$(shasum -a 256 "$downloaded_dmg" | cut -d' ' -f1)
if [[ "$sha256" != "$local_sha256" ]]; then
    echo "Public DMG differs from the local release artifact." >&2
    exit 1
fi
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' \
    "$app/Contents/Info.plist")
cat >"$output" <<RUBY
cask "kodosi-desktop" do
  version "$version"
  sha256 "$sha256"

  url "$KODOSI_DOWNLOAD_URL"
  name "Kodosi Desktop"
  desc "Local terminals with trusted device and friend access"
  homepage "https://github.com/johnkozaris/Kodosi"

  depends_on arch: :arm64
  depends_on macos: ">= $KODOSI_MACOS_DEPLOYMENT_TARGET"

  app "KodosiDesktop.app"
  binary "KodosiDesktop.app/Contents/MacOS/KodosiDesktop", target: "kodosi"

  uninstall quit: "com.kodosi.desktop"
end
RUBY
ruby -c "$output"
if grep -Eq '^[[:space:]]*(zap|trash|rmdir)[[:space:]]' "$output" >/dev/null; then
    echo "The cask must not remove device enrollment under ~/.kodosi" >&2
    exit 1
fi
echo "$output"

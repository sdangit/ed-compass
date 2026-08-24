#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
APP_DIR="$REPO_DIR/dist/ED Compass.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
SOURCE_ICON="$REPO_DIR/assets/ed-compass.ico"
PLIST="$CONTENTS_DIR/Info.plist"

VERSION=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$REPO_DIR/Cargo.toml" | head -n 1)
if [ -z "$VERSION" ]; then
    echo "Could not read the package version from Cargo.toml" >&2
    exit 1
fi

cd "$REPO_DIR"
cargo build --release --locked

if ! file "$REPO_DIR/target/release/ed-compass" | grep -q 'arm64'; then
    echo "The release binary is not Apple Silicon arm64" >&2
    exit 1
fi

mkdir -p "$REPO_DIR/dist" "$REPO_DIR/target/macos-package"
if [ -e "$APP_DIR" ]; then
    rm -rf "$APP_DIR"
fi
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$REPO_DIR/target/release/ed-compass" "$MACOS_DIR/ed-compass"
chmod 755 "$MACOS_DIR/ed-compass"
cp "$SCRIPT_DIR/Info.plist" "$PLIST"
cp "$SCRIPT_DIR/Credits.rtf" "$RESOURCES_DIR/Credits.rtf"
cp "$REPO_DIR/LICENSE" "$RESOURCES_DIR/LICENSE.txt"

/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$PLIST"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$PLIST"

# The Windows resource already contains its native 16–256 px variants. `sips`
# preserves those more faithfully than upscaling the largest one into a
# synthetic 1024 px iconset, and produces a valid ICNS understood by Finder.
sips -s format icns "$SOURCE_ICON" --out "$RESOURCES_DIR/ed-compass.icns" >/dev/null

plutil -lint "$PLIST"
codesign --force --deep --sign - "$APP_DIR"
codesign --verify --deep --strict "$APP_DIR"

echo "Packaged ED Compass $VERSION at:"
echo "$APP_DIR"

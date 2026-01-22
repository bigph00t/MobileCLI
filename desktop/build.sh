#!/bin/bash
# MobileCLI Desktop - Build Script
# Use this script to build the desktop app correctly.
#
# IMPORTANT: Always use this script or `npx tauri build` instead of `cargo build`
# The `cargo build` command doesn't embed frontend assets, causing the app to fail
# with "Could not connect to localhost: Connection refused" on startup.

set -e

cd "$(dirname "$0")"

echo "Building MobileCLI Desktop..."
echo ""

# Build with Tauri (this runs npm build first, then cargo build, then bundles)
npx tauri build

echo ""
VERSION=$(node -p "require('./src-tauri/tauri.conf.json').version")

echo "Build complete! Bundles available at:"
echo "  - macOS:   src-tauri/target/release/bundle/dmg/"
echo "  - Windows: src-tauri/target/release/bundle/nsis/"
echo "  - Linux:   src-tauri/target/release/bundle/deb/ and appimage/"
echo ""
echo "Version: $VERSION"

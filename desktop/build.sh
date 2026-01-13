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
echo "Build complete! Bundles available at:"
echo "  - AppImage: src-tauri/target/release/bundle/appimage/MobileCLI_0.1.0_amd64.AppImage"
echo "  - DEB:      src-tauri/target/release/bundle/deb/MobileCLI_0.1.0_amd64.deb"
echo "  - RPM:      src-tauri/target/release/bundle/rpm/MobileCLI-0.1.0-1.x86_64.rpm"

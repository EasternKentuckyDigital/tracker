#!/bin/bash

set -euo pipefail

if [[ -z "${DEVELOPER_DIR:-}" && -d "/Applications/Xcode.app/Contents/Developer" ]]; then
    export DEVELOPER_DIR="/Applications/Xcode.app/Contents/Developer"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PACKAGE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
REPOSITORY_DIR="$(cd "${PACKAGE_DIR}/../.." && pwd)"
OUTPUT_DIR="${OUTPUT_DIR:-${PACKAGE_DIR}/dist}"
APP_BUNDLE="${OUTPUT_DIR}/Tracker.app"
IDENTITY="${CODESIGN_IDENTITY:--}"
SANDBOXED="${SANDBOXED:-1}"
ARCHITECTURE="$(uname -m)"

case "${ARCHITECTURE}" in
    arm64)
        RUST_TARGET="aarch64-apple-darwin"
        ;;
    x86_64)
        RUST_TARGET="x86_64-apple-darwin"
        ;;
    *)
        echo "Unsupported macOS architecture: ${ARCHITECTURE}" >&2
        exit 1
        ;;
esac

cargo build \
    --manifest-path "${REPOSITORY_DIR}/Cargo.toml" \
    --release \
    --locked \
    --target "${RUST_TARGET}"

swift build \
    --package-path "${PACKAGE_DIR}" \
    --configuration release \
    --arch "${ARCHITECTURE}"

SWIFT_BIN_DIR="$(
    swift build \
        --package-path "${PACKAGE_DIR}" \
        --configuration release \
        --arch "${ARCHITECTURE}" \
        --show-bin-path
)"
RUST_BINARY="${REPOSITORY_DIR}/target/${RUST_TARGET}/release/tracker"

mkdir -p "${OUTPUT_DIR}"
if [[ -e "${APP_BUNDLE}" ]]; then
    rm -rf "${APP_BUNDLE}"
fi
mkdir -p "${APP_BUNDLE}/Contents/MacOS" "${APP_BUNDLE}/Contents/Resources"

install -m 755 "${SWIFT_BIN_DIR}/TrackerMac" "${APP_BUNDLE}/Contents/MacOS/TrackerMac"
install -m 755 "${RUST_BINARY}" "${APP_BUNDLE}/Contents/MacOS/tracker"
install -m 644 "${PACKAGE_DIR}/Distribution/Info.plist" "${APP_BUNDLE}/Contents/Info.plist"

SIGN_TIMESTAMP=(--timestamp)
if [[ "${IDENTITY}" == "-" ]]; then
    SIGN_TIMESTAMP=(--timestamp=none)
fi

if [[ "${SANDBOXED}" == "1" ]]; then
    codesign \
        --force \
        --sign "${IDENTITY}" \
        --identifier "digital.easternkentucky.tracker.helper" \
        --options runtime \
        "${SIGN_TIMESTAMP[@]}" \
        --entitlements "${PACKAGE_DIR}/Distribution/TrackerHelper.entitlements" \
        "${APP_BUNDLE}/Contents/MacOS/tracker"
    codesign \
        --force \
        --sign "${IDENTITY}" \
        --identifier "digital.easternkentucky.tracker" \
        --options runtime \
        "${SIGN_TIMESTAMP[@]}" \
        --entitlements "${PACKAGE_DIR}/Distribution/TrackerMac.entitlements" \
        "${APP_BUNDLE}"
else
    codesign \
        --force \
        --sign "${IDENTITY}" \
        --identifier "digital.easternkentucky.tracker.helper" \
        --options runtime \
        "${SIGN_TIMESTAMP[@]}" \
        "${APP_BUNDLE}/Contents/MacOS/tracker"
    codesign \
        --force \
        --sign "${IDENTITY}" \
        --identifier "digital.easternkentucky.tracker" \
        --options runtime \
        "${SIGN_TIMESTAMP[@]}" \
        "${APP_BUNDLE}"
fi

codesign --verify --deep --strict --verbose=2 "${APP_BUNDLE}"
echo "Built ${APP_BUNDLE}"

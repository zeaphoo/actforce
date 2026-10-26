#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="$ROOT_DIR/dist-musl-static"

RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"
QSV_ALL_FEATURES="${QSV_ALL_FEATURES:-0}"
QSV_FEATURES="${QSV_FEATURES:-feature_capable,apply,fetch,foreach,geocode,polars,to}"
EFFECTIVE_QSV_VERSION="${QSV_VERSION:-12.0.0}"
CACHE_BUST="${CACHE_BUST:-$(date +%s)}"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"
CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}"
CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-false}"

if [ "$QSV_ALL_FEATURES" = "1" ] && [ "${RUST_TOOLCHAIN:-}" = "stable" ]; then
  RUST_TOOLCHAIN="nightly"
fi

echo "Building qsv (musl static) with:"
echo "  RUST_TOOLCHAIN=$RUST_TOOLCHAIN"
echo "  QSV_VERSION=$EFFECTIVE_QSV_VERSION"
echo "  QSV_ALL_FEATURES=$QSV_ALL_FEATURES"
echo "  QSV_FEATURES=$QSV_FEATURES"
echo "  CACHE_BUST=$CACHE_BUST"
echo "  CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS"
echo "  CARGO_PROFILE_RELEASE_CODEGEN_UNITS=$CARGO_PROFILE_RELEASE_CODEGEN_UNITS"
echo "  CARGO_PROFILE_RELEASE_LTO=$CARGO_PROFILE_RELEASE_LTO"

build_one() {
  local platform="$1"   # linux/amd64 or linux/arm64
  local arch="$2"       # amd64 or arm64

  rm -rf "$DIST_DIR/$arch"
  mkdir -p "$DIST_DIR/$arch"

  docker buildx build \
    --platform "$platform" \
    --target export \
    --build-arg QSV_VERSION="$EFFECTIVE_QSV_VERSION" \
    --build-arg RUST_TOOLCHAIN="$RUST_TOOLCHAIN" \
    --build-arg QSV_ALL_FEATURES="$QSV_ALL_FEATURES" \
    --build-arg QSV_FEATURES="$QSV_FEATURES" \
    --build-arg CACHE_BUST="$CACHE_BUST" \
    --build-arg CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS" \
    --build-arg CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" \
    --build-arg CARGO_PROFILE_RELEASE_LTO="$CARGO_PROFILE_RELEASE_LTO" \
    -f "$ROOT_DIR/Dockerfile.musl-static" \
    --output "type=local,dest=$DIST_DIR/$arch" \
    "$ROOT_DIR"

  if [ ! -f "$DIST_DIR/$arch/qsv" ]; then
    echo "ERROR: expected $DIST_DIR/$arch/qsv not found" >&2
    exit 1
  fi
  chmod +x "$DIST_DIR/$arch/qsv"
  echo "Built: $DIST_DIR/$arch/qsv"
  file "$DIST_DIR/$arch/qsv" || true
}

# By default build arm64 to match the project's docker target.
# Uncomment amd64 if you need it.
build_one linux/amd64 amd64
build_one linux/arm64 arm64

echo "Done. Binaries are in: $DIST_DIR/{amd64,arm64}/qsv"

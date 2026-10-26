#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="$ROOT_DIR/dist"

# Build configuration
# - RUST_TOOLCHAIN: stable|nightly (default: stable)
# - QSV_ALL_FEATURES: 0|1 (default: 0)
# - QSV_FEATURES: comma-separated feature list (default: empty)
#
# If you hit errors like "#![feature] may not be used on the stable release channel",
# try:
#   export RUST_TOOLCHAIN=nightly
#   export QSV_ALL_FEATURES=1

RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"
QSV_ALL_FEATURES="${QSV_ALL_FEATURES:-0}"
QSV_FEATURES="${QSV_FEATURES:-feature_capable,apply,fetch,foreach,geocode,polars,to}"
EFFECTIVE_QSV_VERSION="${QSV_VERSION:-12.0.0}"
CACHE_BUST="${CACHE_BUST:-$(date +%s)}"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"
CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}"
CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-false}"

# If all-features is requested, qsv may pull in nightly-gated deps (e.g., foldhash).
# Default to nightly in that case unless the user explicitly set RUST_TOOLCHAIN.
if [ "$QSV_ALL_FEATURES" = "1" ] && [ "${RUST_TOOLCHAIN:-}" = "stable" ]; then
  RUST_TOOLCHAIN="nightly"
fi

echo "Building qsv with:"
echo "  RUST_TOOLCHAIN=$RUST_TOOLCHAIN"
echo "  QSV_VERSION=$EFFECTIVE_QSV_VERSION"
echo "  QSV_ALL_FEATURES=$QSV_ALL_FEATURES"
echo "  QSV_FEATURES=$QSV_FEATURES"
echo "  CACHE_BUST=$CACHE_BUST"
echo "  CARGO_BUILD_JOBS=$CARGO_BUILD_JOBS"
echo "  CARGO_PROFILE_RELEASE_CODEGEN_UNITS=$CARGO_PROFILE_RELEASE_CODEGEN_UNITS"
echo "  CARGO_PROFILE_RELEASE_LTO=$CARGO_PROFILE_RELEASE_LTO"

# Ensure buildx is available and a builder exists.
# If your environment doesn't have a buildx builder, create one:
#   docker buildx create --use --name qsv-builder

build_one() {
  local platform="$1"   # linux/amd64 or linux/arm64
  local arch="$2"       # amd64 or arm64

  rm -rf "$DIST_DIR/$arch"
  mkdir -p "$DIST_DIR/$arch"

  docker buildx build \
    --platform "$platform" \
    --build-arg QSV_VERSION="$EFFECTIVE_QSV_VERSION" \
    --build-arg RUST_TOOLCHAIN="$RUST_TOOLCHAIN" \
    --build-arg QSV_ALL_FEATURES="$QSV_ALL_FEATURES" \
    --build-arg QSV_FEATURES="$QSV_FEATURES" \
    --build-arg CACHE_BUST="$CACHE_BUST" \
    --build-arg CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS" \
    --build-arg CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" \
    --build-arg CARGO_PROFILE_RELEASE_LTO="$CARGO_PROFILE_RELEASE_LTO" \
    -f "$ROOT_DIR/Dockerfile" \
    --output "type=local,dest=$DIST_DIR/$arch" \
    "$ROOT_DIR"

  # buildx exports /qsv from the scratch stage into dest/qsv
  if [ ! -f "$DIST_DIR/$arch/qsv" ]; then
    echo "ERROR: expected $DIST_DIR/$arch/qsv not found" >&2
    exit 1
  fi
  chmod +x "$DIST_DIR/$arch/qsv"

  echo "Built: $DIST_DIR/$arch/qsv"

  host_arch="$(uname -m)"
  can_run=0
  case "$host_arch" in
    x86_64|amd64)
      [ "$arch" = "amd64" ] && can_run=1
      ;;
    aarch64|arm64)
      [ "$arch" = "arm64" ] && can_run=1
      ;;
  esac

  if [ "$can_run" = "1" ]; then
    "$DIST_DIR/$arch/qsv" --version || true
  else
    echo "Skipping local execution of $arch binary on host ($host_arch)"
  fi
}

echo "Building qsv with:"
echo "  RUST_TOOLCHAIN=$RUST_TOOLCHAIN"
echo "  QSV_VERSION=$EFFECTIVE_QSV_VERSION"
echo "  QSV_ALL_FEATURES=$QSV_ALL_FEATURES"
echo "  QSV_FEATURES=$QSV_FEATURES"
echo "  CACHE_BUST=$CACHE_BUST"

build_one linux/amd64 amd64
build_one linux/arm64 arm64

echo "Done. Binaries are in: $DIST_DIR/{amd64,arm64}/qsv"

ARG REGISTRY=docker.m.daocloud.io/
#ARG REGISTRY=docker.1ms.run/

FROM --platform=$TARGETPLATFORM ${REGISTRY}rust:1.92-bookworm AS build

ARG TARGETPLATFORM
ARG QSV_VERSION=
ARG RUST_TOOLCHAIN=stable
ARG QSV_ALL_FEATURES=0
ARG QSV_FEATURES=feature_capable
ARG QSV_GIT_REPO=https://github.com/dathere/qsv
ARG QSV_INSTALL_ROOT=/opt/qsv
ARG CACHE_BUST=0
ARG CARGO_BUILD_JOBS=1
ARG CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
ARG CARGO_PROFILE_RELEASE_LTO=false

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      ca-certificates \
      pkg-config \
      libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Optional: install nightly toolchain for builds that require it.
RUN if [ "$RUST_TOOLCHAIN" = "nightly" ]; then \
      rustup toolchain install nightly --profile minimal; \
    fi

# Build/install qsv.
# - If QSV_VERSION is set, install that version.
# - Otherwise install latest compatible with the cargo registry.
RUN echo "CACHE_BUST=$CACHE_BUST" >/dev/null; \
    TOOLCHAIN=""; \
    if [ "$RUST_TOOLCHAIN" = "nightly" ]; then TOOLCHAIN="+nightly"; fi; \
    FEATURES_ARGS=""; \
    if [ "$QSV_ALL_FEATURES" = "1" ]; then \
      FEATURES_ARGS="--all-features"; \
    elif [ -n "$QSV_FEATURES" ]; then \
      FEATURES_ARGS="--features $QSV_FEATURES"; \
    fi; \
    if [ -n "$QSV_VERSION" ]; then \
      (CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS" CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" CARGO_PROFILE_RELEASE_LTO="$CARGO_PROFILE_RELEASE_LTO" \
         cargo $TOOLCHAIN install qsv --bin qsv --locked --jobs "$CARGO_BUILD_JOBS" $FEATURES_ARGS --root "$QSV_INSTALL_ROOT" --git "$QSV_GIT_REPO" --tag "$QSV_VERSION" || \
       CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS" CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" CARGO_PROFILE_RELEASE_LTO="$CARGO_PROFILE_RELEASE_LTO" \
         cargo $TOOLCHAIN install qsv --bin qsv --locked --jobs "$CARGO_BUILD_JOBS" $FEATURES_ARGS --root "$QSV_INSTALL_ROOT" --git "$QSV_GIT_REPO" --tag "v$QSV_VERSION"); \
    else \
      CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS" CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$CARGO_PROFILE_RELEASE_CODEGEN_UNITS" CARGO_PROFILE_RELEASE_LTO="$CARGO_PROFILE_RELEASE_LTO" \
        cargo $TOOLCHAIN install qsv --bin qsv --locked --jobs "$CARGO_BUILD_JOBS" $FEATURES_ARGS --root "$QSV_INSTALL_ROOT" --git "$QSV_GIT_REPO"; \
    fi; \
    echo "Installed files:"; \
    ls -lah "$QSV_INSTALL_ROOT" || true; \
    ls -lah "$QSV_INSTALL_ROOT/bin" || true; \
    test -x "$QSV_INSTALL_ROOT/bin/qsv"

ENV PATH="$QSV_INSTALL_ROOT/bin:$PATH"

# Export binary to a well-known output location for buildx --output=type=local
RUN "$QSV_INSTALL_ROOT/bin/qsv" --version
RUN mkdir -p /out && cp "$QSV_INSTALL_ROOT/bin/qsv" /out/qsv

FROM scratch AS export
COPY --from=build /out/qsv /qsv

#!/bin/bash
set -e

# ActForce Docker 构建脚本

REGISTRY="${REGISTRY:-docker.m.daocloud.io}"
IMAGE_NAME="actforce"
TAG="${TAG:-latest}"

usage() {
    echo "Usage: $0 {build|save|load|push}"
    echo ""
    echo "Commands:"
    echo "  build   - Build the Docker image"
    echo "  save    - Save image to tar file"
    echo "  load    - Load image from tar file"
    echo "  push    - Push image to registry"
    exit 1
}

cmd_build() {
    echo ">>> Building ${IMAGE_NAME}:${TAG} ..."
    docker build \
        --build-arg REGISTRY="${REGISTRY}" \
        -t "${IMAGE_NAME}:${TAG}" \
        -f Dockerfile \
        ../../../
    echo ">>> Build complete: ${IMAGE_NAME}:${TAG}"
}

cmd_save() {
    local output="${IMAGE_NAME}-${TAG}.tar"
    echo ">>> Saving ${IMAGE_NAME}:${TAG} to ${output} ..."
    docker save "${IMAGE_NAME}:${TAG}" | gzip > "${output}"
    echo ">>> Saved to ${output}"
}

cmd_load() {
    local input="${IMAGE_NAME}-${TAG}.tar"
    if [ ! -f "${input}" ]; then
        input="${IMAGE_NAME}-${TAG}.tar.gz"
    fi
    if [ ! -f "${input}" ]; then
        echo "Error: Image file not found"
        exit 1
    fi
    echo ">>> Loading ${input} ..."
    if [[ "${input}" == *.gz ]]; then
        gunzip -c "${input}" | docker load
    else
        docker load -i "${input}"
    fi
    echo ">>> Loaded successfully"
}

cmd_push() {
    echo ">>> Pushing ${IMAGE_NAME}:${TAG} ..."
    docker push "${IMAGE_NAME}:${TAG}"
    echo ">>> Pushed successfully"
}

# 主逻辑
cd "$(dirname "$0")"

case "${1}" in
    build)  cmd_build ;;
    save)   cmd_save ;;
    load)   cmd_load ;;
    push)   cmd_push ;;
    *)      usage ;;
esac

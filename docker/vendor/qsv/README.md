# qsv vendor

ActForce 的 Excel/CSV 技能、`common::qsv` 和 agent 执行器都会调用 `qsv`。
这里固定 qsv 版本并保存预编译二进制，供 Docker 镜像和本地运行使用。

## 目录

```text
docker/vendor/qsv/
├── Dockerfile                 # glibc 构建
├── Dockerfile.musl-static     # musl 静态构建
├── build.sh                   # 构建 dist/{amd64,arm64}/qsv
├── build-musl-static.sh       # 构建 dist-musl-static/{amd64,arm64}/qsv
├── dist/                      # glibc 预编译二进制（Git LFS）
│   ├── amd64/qsv
│   └── arm64/qsv
└── dist-musl-static/          # musl 静态预编译二进制（Git LFS）
    ├── amd64/qsv
    └── arm64/qsv
```

## 构建

需要 Docker buildx：

```bash
cd docker/vendor/qsv

# glibc 版本
./build.sh

# musl 静态版本
./build-musl-static.sh
```

默认构建 qsv `12.0.0`，可通过 `QSV_VERSION` 覆盖。

## Git LFS

`dist/` 和 `dist-musl-static/` 下的二进制由 Git LFS 管理。克隆后执行：

```bash
git lfs pull
```

否则 Dockerfile 里的 `COPY .../dist/${TARGETARCH}/qsv` 会拿到 LFS pointer。

## ActForce Docker 使用

`distribution/docker/Dockerfile` 会按 `TARGETARCH` 复制：

```text
docker/vendor/qsv/dist/${TARGETARCH}/qsv -> /app/bin/qsv
```

并把 `/app/bin` 加入 `PATH`，保证 `qsv` 可被 `which("qsv")` 和 agent bash 找到。

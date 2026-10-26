# qsv vendor

ActForce 的 Excel/CSV 技能、`common::qsv` 和 agent 执行器都会调用 `qsv`。
这里固定 qsv 版本和构建脚本，二进制按需本地编译，**不提交到 Git**。

## 目录

```text
distribution/docker/vendor/qsv/
├── Dockerfile                 # glibc 构建
├── Dockerfile.musl-static     # musl 静态构建
├── build.sh                   # 构建 dist/{amd64,arm64}/qsv
├── build-musl-static.sh       # 构建 dist-musl-static/{amd64,arm64}/qsv
└── dist/                      # 本地构建产物（已 gitignore）
    ├── amd64/qsv
    └── arm64/qsv
```

## 构建

需要 Docker buildx：

```bash
cd distribution/docker/vendor/qsv

# glibc 版本
./build.sh

# musl 静态版本
./build-musl-static.sh
```

默认构建 qsv `12.0.0`，可通过 `QSV_VERSION` 覆盖。

`dist/` 和 `dist-musl-static/` 是本地产物，已在 `.gitignore` 中忽略。

## ActForce Docker 使用

Docker 构建前先生成对应架构的 qsv：

```bash
cd distribution/docker/vendor/qsv
./build.sh
```

`distribution/docker/Dockerfile` 会按 `TARGETARCH` 复制：

```text
distribution/docker/vendor/qsv/dist/${TARGETARCH}/qsv -> /app/bin/qsv
```

并把 `/app/bin` 加入 `PATH`，保证 `qsv` 可被 `which("qsv")` 和 agent bash 找到。
`distribution/docker/build.sh` 会检查 qsv 是否存在，缺失时给出明确提示。

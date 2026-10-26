# ============================================================
# ActForce Dockerfile — 单租户简化版本
# 使用 SQLite，无需 PostgreSQL
# ============================================================

# ---------- 构建参数 ----------
ARG REGISTRY=docker.m.daocloud.io/
ARG TARGETARCH=amd64

# ---------- 构建阶段：Rust 应用 ----------
FROM ${REGISTRY}rust:1.92-bookworm AS rust-build

WORKDIR /app

# 复制 workspace 配置
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# 构建 actforce
RUN cargo build --release -p actforce

# ---------- 运行阶段 ----------
FROM ${REGISTRY}debian:bookworm-slim
ARG TARGETARCH=amd64

WORKDIR /app

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 复制二进制
COPY --from=rust-build /app/target/release/actforce /app/bin/actforce

# qsv：Excel/CSV 技能和 agent 执行器依赖
# 构建前先运行 distribution/docker/vendor/qsv/build.sh 生成 dist/${TARGETARCH}/qsv
COPY distribution/docker/vendor/qsv/dist/${TARGETARCH}/qsv /app/bin/qsv
RUN chmod +x /app/bin/qsv && /app/bin/qsv --version

# 创建数据目录
RUN mkdir -p /app/data

# 环境变量
ENV PATH="/app/bin:${PATH}"
ENV ACTFORCE_DATA_ROOT=/app/data
ENV ACTFORCE_DEBUG=false

# 暴露端口
EXPOSE 5020

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD curl -f http://127.0.0.1:5020/health || exit 1

# 启动
CMD ["/app/bin/actforce"]

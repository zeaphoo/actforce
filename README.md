# ActForce

ActForce 开源单用户版：通用授权层 + 业务引擎 + 社区版二进制 + 公开 Web UI。

产品关系：

```text
actforce（免费入门，本仓库）
  └─→ actforce-super（私有企业版，独立仓库，依赖本仓库）
         └─→ actforce-mega（多用户/团队版）
```

## 仓库结构

```text
crates/
  entitlement/     通用授权层（Entitlements / Provider / CommunityProvider）
  actforce-core/   业务引擎（auth / chat / skills / inference / file-service 接入）
  actforce/        社区版二进制，注入 CommunityProvider
  common/          通用工具
  agent-core/      Agent 引擎
  file-service/    文件服务
web/               公开 Web UI（AppShell + Entitlements）
packages/
  chat-ui/         共享聊天 UI
  file-manager/    共享文件管理 UI
```

## 运行

### 后端

```bash
cargo run -p actforce
# http://127.0.0.1:5020/health
# GET /api/entitlements
```

环境变量：

```bash
ACTFORCE_HOST=0.0.0.0 \
ACTFORCE_PORT=5020 \
ACTFORCE_DATA_ROOT=./data \
cargo run -p actforce
```

### 前端

```bash
cd web
pnpm install
pnpm dev     # http://127.0.0.1:5193，/api 代理到 5020
pnpm build   # 产物输出到 web/dist
```

后端从仓库根目录读取 `web/dist`。

## 与 ActForce-Super 的关系

- 本仓库只包含开源/公共实现，不包含 license 管理、私有技能、企业 UI。
- 私有 `actforce_super` 通过 git 依赖本仓库的 `actforce-core` / `entitlement`，不复制公共代码。
- 依赖方向只能是 `actforce-super → actforce`，不反向。

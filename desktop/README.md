# ActForce Desktop

ActForce Web UI 的 Electron 桌面壳。

## 开发

```bash
cd desktop
pnpm install
pnpm dev:actforce        # 或 pnpm dev
```

默认加载 `configs/actforce.json` 中的 `defaultUrl`，可用环境变量覆盖：

```bash
WEB_APP_URL=http://127.0.0.1:5020 pnpm dev
```

## 构建

```bash
pnpm build:actforce
```

产物输出到 `dist/actforce/`。

## 配置

`configs/actforce.json` 定义产品名、窗口尺寸、主题色和默认 URL。新增产品时添加同名配置即可，构建脚本通过 `--config=<id>` 选择。

# ofd-file

OFD (Open Fixed-layout Document, 国标 GB/T 33190-2016) 解析与转换库。**纯 Rust 实现**，无 C/C++ 依赖。

OFD 是国内常用的版式文档格式（典型场景：电子发票、公文）。本 crate 提供：

- 解析 OFD 容器（`OFD.xml`、`Document.xml`、`Pages/*/Content.xml`、`PublicRes.xml`、`DocumentRes.xml`、`Annotations.xml` 等）
- 将 OFD 文本内容导出为 **Markdown**
- 将 OFD 导出为**矢量 PDF**（通过 [`krilla`](https://github.com/LaurenzV/krilla)：矢量路径 + 文本 + 位图 + 印章）
- 将 OFD 页面栅格化为 **PNG**（通过 [`vello_cpu`](https://github.com/linebender/vello)：纯 CPU，SIMD 加速，无 GPU/窗口依赖）
- 提取 OFD 中的图片 / 印章资源

## Feature flags

| feature | 默认 | 作用 |
|---|---|---|
| `pdf` | ✅ | `OfdDocument::to_pdf*` → `krilla` |
| `render-cpu` | ✅ | `OfdDocument::to_png*` → `vello_cpu` |
| `cli` | ✅ | `ofd-file` CLI |

参考实现：`rofd` · `ofdrs` · `easyofd`。

> 注：OFD 规范非常庞大，本实现覆盖常见的 `TextObject` / `ImageObject` /
> `PathObject`（M/L/Q/B/C/A），满足电子发票 / 公文等场景。PNG 渲染当前**不包含文字**
> （OFD 用 `DeltaX` 显式定位字形，需要独立的 shaping 通路，后续版本补齐）；
> PDF 渲染则通过 `krilla` 的 `simple-text` 完整支持中文与复杂脚本。
> 数字签章校验、CGM 矢量印章等高级特性暂不支持。

## 使用

```rust
use ofd_file::OfdDocument;

let doc = OfdDocument::open("invoice.ofd")?;

// 导出为 Markdown
let md = doc.to_markdown()?;
std::fs::write("invoice.md", md)?;

// 导出为 PDF
doc.to_pdf_file("invoice.pdf")?;
```

## 命令行

```bash
ofd-file info    invoice.ofd
ofd-file to-md   invoice.ofd -o invoice.md
ofd-file to-pdf  invoice.ofd -o invoice.pdf [--font /path/to/NotoSansCJK.ttc]
ofd-file to-png  invoice.ofd -o invoice --dpi 200
```

CJK 回退字体查找顺序：

1. `--font` / `PdfOptions::fallback_font`
2. `$OFD_FILE_CJK_FONT`
3. 常见系统路径（Noto CJK / WenQuanYi / 微软雅黑 / PingFang 等）

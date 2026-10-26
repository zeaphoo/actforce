# AgentDoc

文档格式转换命令行工具，支持多种文档格式的转换和处理。

## 功能

### 当前支持

- **CHM 文件提取**: 将 CHM (Compiled HTML Help) 文件提取为目录结构，包含 HTML 文件和附件

### 计划支持

- PDF 转换
- DOCX 转换
- Markdown 转换
- EPUB 转换

## 安装

```bash
# 构建项目
cargo build --release -p agent-doc

# 二进制文件位于
# target/release/agent-doc
```

## 使用方法

### 提取 CHM 文件

将 CHM 文件提取到目录（包含 HTML 和附件）：

```bash
# 基本用法（输出到与 CHM 同名的目录）
agent-doc extract document.chm

# 指定输出目录
agent-doc extract document.chm -o output_dir

# 启用详细日志
agent-doc extract document.chm -v
```

### 列出 CHM 文件内容

查看 CHM 文件包含的所有文件：

```bash
agent-doc list document.chm
```

## 命令行选项

### 全局选项

- `-v, --verbose`: 启用详细日志输出
- `-h, --help`: 显示帮助信息
- `-V, --version`: 显示版本信息

### extract 命令

- `<CHM_FILE>`: CHM 文件路径（必需）
- `-o, --output <OUTPUT_DIR>`: 输出目录路径（可选，默认为与 CHM 文件同名的目录）

### list 命令

- `<CHM_FILE>`: CHM 文件路径（必需）

## 输出示例

提取完成后会显示统计信息：

```
提取完成！
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
HTML 文件:    156
其他文件:     43
总文件数:     199
总大小:       12.34 MB
输出目录:     "document"
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

## 技术实现

- 使用 [chmlib](https://crates.io/crates/chmlib) Rust 绑定解析 CHM 文件
- 基于 CHMLib C 库（LGPL-2.0）
- 支持提取所有文件类型：HTML、图片、CSS、JavaScript 等附件

## 开发

```bash
# 开发模式构建
cargo build -p agent-doc

# 运行测试
cargo test -p agent-doc

# 运行（开发模式）
cargo run -p agent-doc -- extract test.chm
```

## 许可证

UNLICENSED

## 参考项目

- [arCHMage](https://github.com/dottedmag/archmage) - Python 实现的 CHM 转换工具

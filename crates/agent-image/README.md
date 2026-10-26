# agent-image

图像处理工具，从 Python 端 `image_spliter.py`、`image_tools.py`、`pdf2image.py` 迁移而来。

同时产出 **library**（供 Rust 端直接调用）和 **CLI 二进制**（供 Python 端 subprocess 调用）。

## 编译

```bash
# 基础编译（不含 PDF 功能）
cargo build -p agent-image

# 含 PDF 转图片功能（需系统安装 poppler-glib / cairo 开发库）
cargo build -p agent-image --features pdf
```

> PDF feature 依赖系统库 `libpoppler-glib` 和 `libcairo`，编译前需安装：
> - Ubuntu/Debian: `apt install libpoppler-glib-dev libcairo2-dev`
> - CentOS/RHEL: `yum install poppler-glib-devel cairo-devel`

## CLI 使用

### split — 图片切分

```bash
# 按空白区域自动切分（默认方式，适合长截图/长表格）
agent-image split -i long_screenshot.png

# 按宽高比切分
agent-image split -i tall_image.png -m ratio --aspect-threshold 3.0

# 按字节大小切分（每份不超过 5MB）
agent-image split -i huge_image.png -m size --max-bytes 5242880

# 指定输出目录
agent-image split -i input.png -o /tmp/split_parts/
```

**参数说明：**

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `-i, --input` | (必填) | 输入图片路径 |
| `-m, --method` | `blank` | 切分方式：`blank`（空白区域）、`ratio`（宽高比）、`size`（字节大小） |
| `-o, --output-dir` | 同目录 `split_output/` | 输出目录 |
| `--min-gap-height` | `30` | [blank] 最小空白区域高度（像素） |
| `--white-thresh` | `250` | [blank] 白色阈值（0-255），灰度值 >= 此值视为白色 |
| `--blank-row-ratio` | `0.95` | [blank] 一行白色像素占比超过此值视为空白行 |
| `--aspect-threshold` | `2.5` | [ratio] 高/宽超过此值才切分 |
| `--max-bytes` | `10485760` | [size] 每份最大字节数（默认 10MB） |

**输出：** stdout 输出 JSON 数组，包含所有切分文件的路径：
```json
["/tmp/split_parts/long_screenshot_part_0001.png", "/tmp/split_parts/long_screenshot_part_0002.png"]
```

### transform — 图片变换

```bash
# 旋转 90 度
agent-image transform -i photo.jpg --rotate 90

# 按短边压缩到 1200px
agent-image transform -i large.png --short-side 1200

# 裁白边
agent-image transform -i scanned.png --crop-white

# 组合：旋转 + 裁白边 + 压缩
agent-image transform -i raw.png --rotate 90 --crop-white --short-side 1200 -o output.png
```

**参数说明：**

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `-i, --input` | (必填) | 输入图片路径 |
| `--rotate` | `0` | 旋转角度：0 / 90 / 180 / 270 |
| `--short-side` | `0` | 按短边压缩到的像素值，0 表示不压缩 |
| `--crop-white` | `false` | 是否裁白边 |
| `--crop-white-thresh` | `240` | 裁白边白色阈值（0-255） |
| `--crop-padding` | `2` | 裁白边后保留的边缘像素 |
| `-o, --output` | 覆盖原图 | 输出路径，扩展名决定格式（.png / .jpg） |

**处理流水线：** 旋转 → 裁白边 → 压缩（按此顺序执行）

### pdf2img — PDF 转图片（需 `pdf` feature）

```bash
# 转换全部页面
agent-image pdf2img -i document.pdf

# 指定 DPI 和页码范围
agent-image pdf2img -i document.pdf --dpi 200 --pages 1-5

# 不裁白边
agent-image pdf2img -i document.pdf --crop-white false

# 指定输出目录
agent-image pdf2img -i document.pdf -o /tmp/pdf_pages/
```

**参数说明：**

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `-i, --input` | (必填) | 输入 PDF 路径 |
| `--dpi` | `288` | 渲染 DPI |
| `--pages` | 全部页 | 页码范围，如 `1-5` 或 `3` |
| `--crop-white` | `true` | 是否裁白边 |
| `--crop-white-thresh` | `240` | 裁白边白色阈值 |
| `--crop-padding` | `2` | 裁白边后保留的边缘像素 |
| `-o, --output-dir` | 同目录 `pdf_output/` | 输出目录 |

**输出：** stdout 输出 JSON 数组，包含所有页面图片的路径：
```json
["/tmp/pdf_pages/document_page_0001.png", "/tmp/pdf_pages/document_page_0002.png"]
```

### info — 图片信息

```bash
agent-image info -i photo.jpg
```

**输出：**
```json
{
  "path": "photo.jpg",
  "width": 1920,
  "height": 1080,
  "format": "jpg"
}
```

## Rust Library 使用

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
agent-image = { path = "crates/agent-image" }

# 如需 PDF 功能
# agent-image = { path = "crates/agent-image", features = ["pdf"] }
```

### 图片切分

```rust
use agent_image::split::{self, BlankRegionParams};

let img = split::read_image("long_screenshot.png")?;

// 自动切分（高/宽 > 3 时尝试按空白区域切分）
let parts = split::auto_split(&img, &BlankRegionParams::default());

// 按空白区域切分
let parts = split::split_by_blank_regions(&img, &BlankRegionParams {
    min_gap_height: 40,
    white_thresh: 245,
    blank_row_ratio: 0.90,
});

// 按宽高比切分
let parts = split::split_by_ratio(&img, 3.0);

// 按字节大小切分
let parts = split::split_by_byte_size(&img, 5 * 1024 * 1024);

// 保存结果
let paths = split::save_parts(&parts, Path::new("input.png"), Path::new("output/"))?;
```

### 图片变换

```rust
use agent_image::transform::{self, RotationAngle, CropWhiteParams, PreprocessParams};

let img = split::read_image("photo.jpg")?;

// 单独操作
let rotated = transform::rotate(&img, RotationAngle::Deg90);
let cropped = transform::crop_white_border(&img, &CropWhiteParams::default());
let resized = transform::resize_by_short_side(&img, 1200);

// 预处理流水线：旋转 → 裁白边 → 压缩
let params = PreprocessParams {
    rotation: RotationAngle::Deg90,
    target_short_side: 1200,
    crop_white: true,
    crop_white_params: CropWhiteParams {
        white_thresh: 240,
        padding: 2,
    },
};
let result = transform::preprocess(&img, &params);
transform::save_image(&result, Path::new("output.png"), transform::OutputFormat::Png)?;

// 编码为字节
let png_bytes = transform::encode_image(&result, transform::OutputFormat::Png)?;
```

### PDF 转图片（需 `pdf` feature）

```rust
use agent_image::pdf2img::{self, PdfToImageOptions};

let opts = PdfToImageOptions {
    dpi: 288,
    first_page: Some(1),
    last_page: Some(5),
    crop_border: true,
    white_thresh: 240,
    padding: 2,
};
let images: Vec<Vec<u8>> = pdf2img::pdf_to_images(Path::new("doc.pdf"), &opts)?;

let page_count = pdf2img::get_pdf_page_count(Path::new("doc.pdf"))?;
```

## Python 端调用

```python
import subprocess, json

def split_image(input_path: str, method: str = "blank") -> list[str]:
    result = subprocess.run(
        ["agent-image", "split", "-i", input_path, "-m", method],
        capture_output=True, text=True
    )
    return json.loads(result.stdout)

def preprocess_image(input_path: str, short_side: int = 1200, crop_white: bool = True) -> None:
    args = ["agent-image", "transform", "-i", input_path,
            "--short-side", str(short_side)]
    if crop_white:
        args.append("--crop-white")
    subprocess.run(args, check=True)

def pdf_to_images(input_path: str, dpi: int = 288) -> list[str]:
    result = subprocess.run(
        ["agent-image", "pdf2img", "-i", input_path, "--dpi", str(dpi)],
        capture_output=True, text=True
    )
    return json.loads(result.stdout)
```

## 与 Python 原始工具的对应关系

| Python 文件 | Rust 模块 | CLI 子命令 |
|-------------|-----------|------------|
| `image_spliter.py` — `ImageSplitter` | `split.rs` | `split` |
| `image_tools.py` — `rotate_image` | `transform.rs` — `rotate` | `transform --rotate` |
| `image_tools.py` — `resize_by_short_side` | `transform.rs` — `resize_by_short_side` | `transform --short-side` |
| `image_tools.py` — `crop_white_border` | `transform.rs` — `crop_white_border` | `transform --crop-white` |
| `image_tools.py` — `image_to_bytes` | `transform.rs` — `encode_image` | (library only) |
| `pdf2image.py` — `pdf2imagesbytes` | `pdf2img.rs` — `pdf_to_images` | `pdf2img` |
| `pdf2image.py` — `get_pdf_page_count` | `pdf2img.rs` — `get_pdf_page_count` | (library only) |

## 项目结构

```
crates/agent-image/
├── Cargo.toml          # [lib] + [[bin]]，pdf feature 可选
└── src/
    ├── lib.rs          # library 入口
    ├── main.rs         # CLI 入口 (clap)
    ├── split.rs        # 图片切分
    ├── transform.rs    # 旋转/压缩/裁白边/预处理流水线
    └── pdf2img.rs      # PDF 转图片 (feature = "pdf")
```

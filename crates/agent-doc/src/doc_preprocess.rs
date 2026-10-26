//! 文档预处理：将非 OOXML 格式转换为 OOXML，以便后续统一处理。
//!
//! 支持的格式：
//! - Microsoft Office 旧版 (97-2003) 二进制：`.doc` / `.xls` / `.ppt`
//! - OpenDocument Format (ODF)：`.odt` / `.ods` / `.odp`
//! - 中文办公格式 (UOF)：`.uot` / `.uof` / `.uos` / `.uop`
//!
//! 流程：
//! 1. 对于 `.doc` / `.xls` / `.ppt`：
//!    - 通过魔数判断：`PK\x03\x04` → OOXML（扩展名写错），复制改名即可
//!    - `\xD0\xCF\x11\xE0...` → OLE2 旧版二进制，调用 LibreOffice 转换
//! 2. 对于 `.odt` / `.ods` / `.odp` / `.uot` 等：
//!    - 直接调用 LibreOffice 转换为对应的 OOXML 格式
//!
//! 返回 [`PreprocessedDoc`]，它持有一个可选的 `_guard`，在 drop 时清理临时文件。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use tracing::{debug, info, warn};

/// 预处理后的文档路径（可能是原路径或临时目录中的新路径）。
pub struct PreprocessedDoc {
    path: PathBuf,
    /// 临时目录守卫：为 `Some` 时，drop 会递归删除该目录。
    _guard: Option<TempDirGuard>,
}

impl PreprocessedDoc {
    /// 返回用于后续处理的路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 如果内部使用了临时文件，则返回 `true`。
    pub fn is_temp(&self) -> bool {
        self._guard.is_some()
    }
}

/// 简单的临时目录 RAII 守卫。
pub struct TempDirGuard {
    dir: PathBuf,
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.dir.exists() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// 识别出的文件实际容器格式（仅用于 MS Office 旧版格式检测）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeContainer {
    /// OOXML：`.docx` / `.xlsx` / `.pptx`（ZIP）
    Ooxml,
    /// OLE2 Compound：`.doc` / `.xls` / `.ppt`（旧版二进制）
    Ole2,
    /// 其他（例如 RTF、HTML 伪装、损坏等）
    Unknown,
}

/// 通过魔数识别文件容器格式。
pub fn sniff_container<P: AsRef<Path>>(path: P) -> Result<OfficeContainer> {
    let mut f = std::fs::File::open(path.as_ref())
        .with_context(|| format!("无法打开文件: {:?}", path.as_ref()))?;
    let mut head = [0u8; 8];
    let n = f.read(&mut head).unwrap_or(0);

    // ODF 格式也是 ZIP，但可以通过内部结构判断
    // 这里先简单判断是否为 ZIP
    if n >= 4 && &head[0..4] == b"PK\x03\x04" {
        // 可能是 OOXML 或 ODF，需要进一步判断
        // 这里暂时返回 Ooxml，具体由扩展名决定
        return Ok(OfficeContainer::Ooxml);
    }

    if n >= 8 && head == [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] {
        return Ok(OfficeContainer::Ole2);
    }

    Ok(OfficeContainer::Unknown)
}

/// 旧版 Microsoft Office 扩展名 → 对应的 OOXML 扩展名映射。
pub fn legacy_ms_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "doc" => Some("docx"),
        "xls" => Some("xlsx"),
        "ppt" => Some("pptx"),
        _ => None,
    }
}

/// ODF 扩展名 → 对应的 OOXML 扩展名映射。
pub fn odf_to_ooxml_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "odt" => Some("docx"),  // ODF Text → DOCX
        "ods" => Some("xlsx"),  // ODF Spreadsheet → XLSX
        "odp" => Some("pptx"),  // ODF Presentation → PPTX
        _ => None,
    }
}

/// UOF (中文办公格式) 扩展名 → 对应的 OOXML 扩展名映射。
pub fn uof_to_ooxml_ext(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "uot" | "uof" => Some("docx"),  // UOF Text → DOCX
        "uos" => Some("xlsx"),           // UOF Spreadsheet → XLSX
        "uop" => Some("pptx"),           // UOF Presentation → PPTX
        _ => None,
    }
}

/// 所有需要转换的扩展名 → 目标 OOXML 扩展名。
fn target_ext_for(ext: &str) -> Option<&'static str> {
    legacy_ms_ext(ext)
        .or_else(|| odf_to_ooxml_ext(ext))
        .or_else(|| uof_to_ooxml_ext(ext))
}

/// 判断扩展名是否属于需要魔数检测的类型（仅 MS Office 旧版）。
fn needs_container_sniff(ext: &str) -> bool {
    legacy_ms_ext(ext).is_some()
}

/// 判断扩展名是否需要直接用 LibreOffice 转换（无需魔数检测）。
fn needs_direct_convert(ext: &str) -> bool {
    odf_to_ooxml_ext(ext).is_some() || uof_to_ooxml_ext(ext).is_some()
}

/// 主入口：处理需要转换的文档格式。
///
/// 支持的格式：
/// - `.doc` / `.xls` / `.ppt`：旧版 MS Office，先魔数检测再决定是否转换
/// - `.odt` / `.ods` / `.odp`：OpenDocument Format，直接转换为 OOXML
/// - `.uot` / `.uof` / `.uos` / `.uop`：中文 UOF 格式，直接转换为 OOXML
///
/// 返回的 [`PreprocessedDoc`] 必须在后续使用结束前保持存活，否则临时文件会被清理。
pub fn preprocess_doc<P: AsRef<Path>>(source: P) -> Result<PreprocessedDoc> {
    let source = source.as_ref();
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // 判断是否需要处理
    let Some(target_ext) = target_ext_for(&ext) else {
        return Ok(PreprocessedDoc { path: source.to_path_buf(), _guard: None });
    };

    // 对于 MS Office 旧版格式，先做魔数检测
    if needs_container_sniff(&ext) {
        let container = sniff_container(source)
            .with_context(|| format!("识别文件格式失败: {:?}", source))?;

        let result = match container {
            OfficeContainer::Ooxml => {
                // 扩展名写错，其实已经是 OOXML。复制到临时目录改名即可。
                info!(
                    "文件 {:?} 扩展名 .{} 但实际为 OOXML，按 .{} 处理",
                    source, ext, target_ext
                );
                let (new_path, guard) = copy_to_temp_with_ext(source, target_ext)?;
                PreprocessedDoc { path: new_path, _guard: Some(guard) }
            }
            OfficeContainer::Ole2 => {
                // 真正的旧版二进制，需要转换
                info!(
                    "检测到旧版 Office 二进制文件 {:?}，使用 LibreOffice 转换为 .{}",
                    source, target_ext
                );
                let (new_path, guard) = convert_with_libreoffice(source, target_ext)?;
                PreprocessedDoc { path: new_path, _guard: Some(guard) }
            }
            OfficeContainer::Unknown => {
                warn!(
                    "文件 {:?} 扩展名 .{} 但既不是 OOXML 也不是 OLE2，保持原样（后续处理可能失败）",
                    source, ext
                );
                PreprocessedDoc { path: source.to_path_buf(), _guard: None }
            }
        };
        return Ok(result);
    }

    // 对于 ODF / UOF 格式，直接转换
    if needs_direct_convert(&ext) {
        if !is_libreoffice_available() {
            warn!(
                "文件 {:?} 需要转换为 OOXML 但 LibreOffice 不可用，保持原样",
                source
            );
            return Ok(PreprocessedDoc { path: source.to_path_buf(), _guard: None });
        }

        info!(
            "检测到 {:?} 格式，使用 LibreOffice 转换为 .{}",
            source, target_ext
        );
        let (new_path, guard) = convert_with_libreoffice(source, target_ext)?;
        return Ok(PreprocessedDoc { path: new_path, _guard: Some(guard) });
    }

    // 其他情况原样返回
    Ok(PreprocessedDoc { path: source.to_path_buf(), _guard: None })
}

/// 在进程临时目录下创建唯一子目录。
fn new_temp_subdir(tag: &str) -> Result<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "agent-doc-{}-{}-{}",
        tag,
        std::process::id(),
        n
    ));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("无法创建临时目录: {:?}", dir))?;
    Ok(dir)
}

/// 复制文件到临时目录并使用新扩展名。
fn copy_to_temp_with_ext(source: &Path, new_ext: &str) -> Result<(PathBuf, TempDirGuard)> {
    let dir = new_temp_subdir("rename")?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("input");
    let target = dir.join(format!("{stem}.{new_ext}"));
    std::fs::copy(source, &target)
        .with_context(|| format!("复制文件失败: {:?} -> {:?}", source, target))?;
    Ok((target, TempDirGuard { dir }))
}

/// 调用 `libreoffice` / `soffice` 将旧版 Office 文档转换为 OOXML。
fn convert_with_libreoffice(source: &Path, target_ext: &str) -> Result<(PathBuf, TempDirGuard)> {
    let bin = locate_libreoffice()
        .ok_or_else(|| anyhow!("未找到 libreoffice / soffice，请先安装 LibreOffice 以转换 .doc/.xls/.ppt"))?;
    debug!("using libreoffice binary: {:?}", bin);

    let out_dir = new_temp_subdir("soffice")?;

    // 先将源文件复制到临时目录，避免 libreoffice 直接操作原文件
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("input");
    let source_ext = source.extension().and_then(|e| e.to_str()).unwrap_or("");
    let local_source = out_dir.join(format!("{stem}.{source_ext}"));
    std::fs::copy(source, &local_source)
        .with_context(|| format!("复制源文件到临时目录失败: {:?} -> {:?}", source, local_source))?;

    let mut cmd = Command::new(&bin);
    cmd.arg("--headless")
        .arg("--norestore")
        .arg("--nologo")
        .arg("--nodefault")
        .arg("--nofirststartwizard")
        .arg("--convert-to")
        .arg(target_ext)
        .arg("--outdir")
        .arg(&out_dir)
        .arg(&local_source);

    // 使用独立的用户配置目录，避免并发实例相互锁定。
    let profile_dir = out_dir.join("lo-profile");
    std::fs::create_dir_all(&profile_dir).ok();
    cmd.arg(format!(
        "-env:UserInstallation=file://{}",
        profile_dir.to_string_lossy()
    ));

    debug!("running: {:?}", cmd);
    let output = cmd
        .output()
        .with_context(|| format!("无法执行 {:?}", bin))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow!(
            "LibreOffice 转换失败 (exit={:?})\nstdout: {}\nstderr: {}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        ));
    }

    // soffice 会以原文件名（改扩展名）写出到 outdir。
    let expected = out_dir.join(format!("{stem}.{target_ext}"));
    if expected.exists() {
        return Ok((expected, TempDirGuard { dir: out_dir }));
    }

    // 有些版本会规范化文件名；兜底扫描 outdir（排除 profile 目录）。
    for entry in std::fs::read_dir(&out_dir)
        .with_context(|| format!("读取 outdir 失败: {:?}", out_dir))?
    {
        let entry = entry?;
        let p = entry.path();
        // 跳过 profile 目录
        if p.file_name().map_or(false, |n| n == "lo-profile") {
            continue;
        }
        if p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case(target_ext))
            == Some(true)
        {
            return Ok((p, TempDirGuard { dir: out_dir }));
        }
    }

    Err(anyhow!(
        "LibreOffice 未产生输出文件（期望: {:?}）\n{}",
        expected,
        String::from_utf8_lossy(&output.stdout)
    ))
}

/// 查找系统上的 LibreOffice 可执行文件。
pub fn locate_libreoffice() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LIBREOFFICE_BIN").map(PathBuf::from) {
        if path.exists() {
            return Some(path);
        }
    }

    // which-style 查找。
    for name in ["soffice", "libreoffice"] {
        if let Some(p) = which_on_path(name) {
            return Some(p);
        }
    }

    // 平台常见安装路径。
    for p in PLATFORM_CANDIDATES {
        let p = Path::new(p);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// LibreOffice 是否可用。
pub fn is_libreoffice_available() -> bool {
    locate_libreoffice().is_some()
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
        // Windows：附加 .exe。
        #[cfg(windows)]
        {
            let cand_exe = dir.join(format!("{name}.exe"));
            if cand_exe.is_file() {
                return Some(cand_exe);
            }
        }
    }
    None
}

const PLATFORM_CANDIDATES: &[&str] = &[
    // Linux
    "/usr/bin/soffice",
    "/usr/bin/libreoffice",
    "/usr/lib/libreoffice/program/soffice",
    "/snap/bin/libreoffice",
    // macOS
    "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    // Windows
    "C:/Program Files/LibreOffice/program/soffice.exe",
    "C:/Program Files (x86)/LibreOffice/program/soffice.exe",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_ooxml_vs_ole2() {
        let dir = new_temp_subdir("test").unwrap();
        let _guard = TempDirGuard { dir: dir.clone() };

        let ooxml = dir.join("a.doc");
        std::fs::write(&ooxml, b"PK\x03\x04rest of zip").unwrap();
        assert_eq!(sniff_container(&ooxml).unwrap(), OfficeContainer::Ooxml);

        let ole = dir.join("b.doc");
        std::fs::write(
            &ole,
            [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0],
        )
        .unwrap();
        assert_eq!(sniff_container(&ole).unwrap(), OfficeContainer::Ole2);

        let unk = dir.join("c.doc");
        std::fs::write(&unk, b"not an office document").unwrap();
        assert_eq!(sniff_container(&unk).unwrap(), OfficeContainer::Unknown);
    }

    #[test]
    fn ms_legacy_extension_mapping() {
        assert_eq!(legacy_ms_ext("doc"), Some("docx"));
        assert_eq!(legacy_ms_ext("XLS"), Some("xlsx"));
        assert_eq!(legacy_ms_ext("ppt"), Some("pptx"));
        assert_eq!(legacy_ms_ext("pptx"), None);
        assert_eq!(legacy_ms_ext("txt"), None);
    }

    #[test]
    fn odf_extension_mapping() {
        assert_eq!(odf_to_ooxml_ext("odt"), Some("docx"));
        assert_eq!(odf_to_ooxml_ext("ods"), Some("xlsx"));
        assert_eq!(odf_to_ooxml_ext("odp"), Some("pptx"));
        assert_eq!(odf_to_ooxml_ext("docx"), None);
    }

    #[test]
    fn uof_extension_mapping() {
        assert_eq!(uof_to_ooxml_ext("uot"), Some("docx"));
        assert_eq!(uof_to_ooxml_ext("uos"), Some("xlsx"));
        assert_eq!(uof_to_ooxml_ext("uop"), Some("pptx"));
        assert_eq!(uof_to_ooxml_ext("uof"), Some("docx"));
    }

    #[test]
    fn handle_ooxml_with_wrong_extension() {
        let dir = new_temp_subdir("case").unwrap();
        let _guard = TempDirGuard { dir: dir.clone() };

        let path = dir.join("mislabelled.doc");
        std::fs::write(&path, b"PK\x03\x04stub").unwrap();

        let doc = preprocess_doc(&path).unwrap();
        assert!(doc.is_temp());
        assert_eq!(
            doc.path().extension().and_then(|e| e.to_str()),
            Some("docx")
        );
        assert!(doc.path().exists());
    }

    #[test]
    fn target_ext_for_all_supported_formats() {
        // 验证所有支持的格式映射
        assert_eq!(target_ext_for("doc"), Some("docx"));
        assert_eq!(target_ext_for("xls"), Some("xlsx"));
        assert_eq!(target_ext_for("ppt"), Some("pptx"));
        assert_eq!(target_ext_for("odt"), Some("docx"));
        assert_eq!(target_ext_for("ods"), Some("xlsx"));
        assert_eq!(target_ext_for("odp"), Some("pptx"));
        assert_eq!(target_ext_for("uot"), Some("docx"));
        assert_eq!(target_ext_for("uos"), Some("xlsx"));
        assert_eq!(target_ext_for("uop"), Some("pptx"));
        assert_eq!(target_ext_for("uof"), Some("docx"));
        assert_eq!(target_ext_for("docx"), None);
        assert_eq!(target_ext_for("txt"), None);
    }
}

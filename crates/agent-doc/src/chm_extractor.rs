use anyhow::{Context, Result};
use chmlib::{ChmFile, Filter, UnitInfo, Continuation};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf, Component};
use tracing::{info, warn, error};

/// CHM 文件提取器
pub struct ChmExtractor {
    chm_file: ChmFile,
}

/// 提取结果统计
#[derive(Debug, Default)]
pub struct ExtractStats {
    pub html_files: u32,
    pub other_files: u32,
    pub total_files: u32,
    pub total_size: u64,
    pub skipped_files: u32,
}

impl ChmExtractor {
    /// 打开 CHM 文件
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let chm_file = ChmFile::open(&path)
            .with_context(|| format!("无法打开 CHM 文件: {:?}", path.as_ref()))?;

        info!("成功打开 CHM 文件: {:?}", path.as_ref());

        Ok(Self { chm_file })
    }

    /// 提取 CHM 文件到指定目录（目录不能已存在，防止覆盖）
    pub fn extract<P: AsRef<Path>>(&mut self, output_dir: P) -> Result<ExtractStats> {
        let output_dir = output_dir.as_ref();

        // 目标目录已存在时拒绝提取，与 archmage 行为一致
        if output_dir.exists() {
            anyhow::bail!(
                "输出目录已存在: {:?}。请删除后重试，或通过 -o 指定其他目录。",
                output_dir
            );
        }

        fs::create_dir_all(output_dir)
            .with_context(|| format!("无法创建输出目录: {:?}", output_dir))?;

        let mut stats = ExtractStats::default();

        info!("开始提取 CHM 文件到: {:?}", output_dir);

        self.chm_file
            .for_each(Filter::FILES, |chm, unit_info| {
                let raw_path = unit_info
                    .path()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                match safe_relative_path(&raw_path) {
                    None => {
                        warn!("跳过不安全路径: {}", raw_path);
                        stats.skipped_files += 1;
                    }
                    Some(rel_path) => {
                        if let Err(e) =
                            extract_single_file(chm, &unit_info, &rel_path, output_dir, &mut stats)
                        {
                            error!("提取文件失败 {}: {}", rel_path.display(), e);
                        }
                    }
                }
                Continuation::Continue
            })
            .map_err(|e| anyhow::anyhow!("枚举 CHM 文件内容失败: {}", e))?;

        info!(
            "提取完成: {} 个 HTML 文件, {} 个其他文件, {} 个跳过, 总计 {} 个",
            stats.html_files, stats.other_files, stats.skipped_files, stats.total_files
        );

        Ok(stats)
    }

    /// 列出 CHM 文件内容（所有条目，包含目录）
    pub fn list_contents(&mut self) -> Result<Vec<String>> {
        let mut contents = Vec::new();

        self.chm_file
            .for_each(Filter::all(), |_chm, unit_info| {
                if let Some(path) = unit_info.path() {
                    contents.push(path.to_string_lossy().to_string());
                }
                Continuation::Continue
            })
            .map_err(|e| anyhow::anyhow!("枚举 CHM 文件内容失败: {}", e))?;

        Ok(contents)
    }
}

/// 将 CHM 内部路径转换为安全的相对路径。
///
/// 规则：
/// - 去掉前导 `/` 或 `\`
/// - 路径不得包含 `..` 组件（防止路径穿越攻击）
/// - 绝对路径组件（根盘符等）直接拒绝
/// - 路径为空或仅为 `/` 时返回 None
fn safe_relative_path(raw: &str) -> Option<PathBuf> {
    // 去掉首个分隔符
    let trimmed = raw.trim_start_matches('/').trim_start_matches('\\');

    if trimmed.is_empty() {
        return None;
    }

    let path = PathBuf::from(trimmed);
    let mut safe = PathBuf::new();

    for component in path.components() {
        match component {
            // 正常路径段，保留
            Component::Normal(seg) => safe.push(seg),
            // 当前目录 `.`，跳过
            Component::CurDir => {}
            // `..`、根目录、盘符 —— 全部拒绝
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return None;
            }
        }
    }

    if safe.as_os_str().is_empty() {
        None
    } else {
        Some(safe)
    }
}

/// 提取单个文件到输出目录
fn extract_single_file<P: AsRef<Path>>(
    chm: &mut ChmFile,
    unit_info: &UnitInfo,
    rel_path: &Path,   // 已经过安全校验的相对路径
    output_dir: P,
    stats: &mut ExtractStats,
) -> Result<()> {
    let output_path = output_dir.as_ref().join(rel_path);

    // 确保父目录存在
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("无法创建目录: {:?}", parent))?;
    }

    // 读取文件内容
    let file_length = unit_info.length() as usize;
    let mut buffer = vec![0u8; file_length];

    chm.read(unit_info, 0, &mut buffer)
        .with_context(|| format!("无法读取文件: {:?}", rel_path))?;

    // 写入文件
    let mut file = fs::File::create(&output_path)
        .with_context(|| format!("无法创建文件: {:?}", output_path))?;

    file.write_all(&buffer)
        .with_context(|| format!("无法写入文件: {:?}", output_path))?;

    // 更新统计
    stats.total_files += 1;
    stats.total_size += buffer.len() as u64;

    let ext = rel_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "html" || ext == "htm" {
        stats.html_files += 1;
    } else {
        stats.other_files += 1;
    }

    info!("提取文件: {} ({} bytes)", rel_path.display(), buffer.len());

    Ok(())
}

/// 根据 CHM 文件路径生成默认输出目录（与 archmage file2dir 逻辑一致）。
///
/// 规则：去掉 `.chm` 后缀后加 `_html`，例如：
/// - `docs.chm`  →  `docs_html`
/// - `manual`    →  `manual_html`
pub fn default_output_dir(chm_path: &Path) -> PathBuf {
    let stem = chm_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let dir_name = format!("{}_html", stem);

    match chm_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(dir_name),
        _ => PathBuf::from(dir_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_path_normal() {
        assert_eq!(
            safe_relative_path("/folder/page.html"),
            Some(PathBuf::from("folder/page.html"))
        );
    }

    #[test]
    fn test_safe_path_root_only() {
        assert_eq!(safe_relative_path("/"), None);
        assert_eq!(safe_relative_path(""), None);
    }

    #[test]
    fn test_safe_path_parent_traversal() {
        assert_eq!(safe_relative_path("/../etc/passwd"), None);
        assert_eq!(safe_relative_path("/a/../../etc/passwd"), None);
        assert_eq!(safe_relative_path("../secret"), None);
    }

    #[test]
    fn test_safe_path_current_dir() {
        // ./page.html 应等效于 page.html
        assert_eq!(
            safe_relative_path("/./page.html"),
            Some(PathBuf::from("page.html"))
        );
    }

    #[test]
    fn test_default_output_dir() {
        assert_eq!(
            default_output_dir(Path::new("docs.chm")),
            PathBuf::from("docs_html")
        );
        assert_eq!(
            default_output_dir(Path::new("/tmp/manual.chm")),
            PathBuf::from("/tmp/manual_html")
        );
        assert_eq!(
            default_output_dir(Path::new("noext")),
            PathBuf::from("noext_html")
        );
    }
}

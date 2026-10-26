use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook, Data, Range, Reader, Xlsx};

pub fn xlsx_to_csv(
    xlsx_path: &Path,
    output_path: &Path,
    sheet_name: Option<&str>,
    encoding_utf8: bool,
) -> Result<PathBuf> {
    if !xlsx_path.exists() {
        return Err(anyhow!("xlsx not found: {}", xlsx_path.display()));
    }
    if xlsx_path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase() != "xlsx" {
        return Err(anyhow!("unsupported excel format (only .xlsx): {}", xlsx_path.display()));
    }

    let mut workbook: Xlsx<_> = open_workbook(xlsx_path).with_context(|| format!("open xlsx failed: {}", xlsx_path.display()))?;

    let chosen_sheet = if let Some(s) = sheet_name {
        s.to_string()
    } else {
        let names = workbook.sheet_names().to_vec();
        names
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("xlsx has no sheets"))?
    };

    let range: Range<Data> = workbook
        .worksheet_range(&chosen_sheet)
        .map_err(|e| anyhow!("read sheet failed: {e}"))?;

    std::fs::create_dir_all(
        output_path
            .parent()
            .ok_or_else(|| anyhow!("invalid output path"))?,
    )?;

    let f = File::create(output_path)?;
    let mut wtr = csv::Writer::from_writer(f);

    for row in range.rows() {
        let mut out: Vec<String> = Vec::with_capacity(row.len());
        for v in row.iter() {
            let s = match v {
                Data::Empty => "".to_string(),
                Data::String(s) => s.to_string(),
                Data::Float(x) => {
                    if x.fract() == 0.0 {
                        format!("{}", *x as i64)
                    } else {
                        format!("{}", x)
                    }
                }
                Data::Int(i) => format!("{}", i),
                Data::Bool(b) => format!("{}", b),
                Data::DateTime(x) => format!("{}", x),
                Data::DateTimeIso(s) => s.to_string(),
                Data::DurationIso(s) => s.to_string(),
                Data::Error(e) => format!("{:?}", e),
            };
            out.push(s);
        }
        wtr.write_record(out)?;
    }

    wtr.flush()?;

    if !encoding_utf8 {
        return Err(anyhow!("only utf-8 output is supported"));
    }

    Ok(output_path.to_path_buf())
}

//! Tiny example: convert an OFD file to Markdown and print to stdout.
//!
//! Run with: `cargo run -p ofd-file --example ofd_to_md -- path/to/file.ofd`

use ofd_file::OfdDocument;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: ofd_to_md <file.ofd>"))?;
    let doc = OfdDocument::open(&path)?;
    println!("{}", doc.to_markdown()?);
    Ok(())
}

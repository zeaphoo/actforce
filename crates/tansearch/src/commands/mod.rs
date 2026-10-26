mod bench;
mod delete;
mod index;
mod infer_schema;
mod inspect;
mod merge;
mod new;
mod search;
mod serve;

use std::path::Path;

use tantivy::Index;

pub fn open_index(path: &Path) -> tantivy::Result<Index> {
    let index = Index::open_in_dir(path)?;
    register_jieba_tokenizer(&index);
    Ok(index)
}

fn register_jieba_tokenizer(index: &Index) {
    index
        .tokenizers()
        .register("jieba", tantivy_jieba::JiebaTokenizer::new());
}

pub use self::bench::run_bench_cli;
pub use self::delete::run_delete_cli;
pub use self::index::run_index_cli;
pub use self::inspect::run_inspect_cli;
pub use self::merge::run_merge_cli;
pub use self::new::run_new_cli;
pub use self::search::run_search_cli;
pub use self::serve::run_serve_cli;

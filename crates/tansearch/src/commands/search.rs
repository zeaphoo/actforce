use clap::ArgMatches;
use std::convert::From;
use std::io::{self, ErrorKind, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process;
use tantivy::aggregation::agg_req::Aggregations;
use tantivy::aggregation::AggregationCollector;
use tantivy::aggregation::AggregationLimitsGuard;
use tantivy::collector::{Count, TopDocs};
use tantivy::query::{EnableScoring, QueryParser};
use tantivy::schema::NamedFieldDocument;
use tantivy::schema::Field;
use tantivy::schema::FieldType;
use tantivy::Document;
use tantivy::Order;
use tantivy::{self, TantivyDocument};
use tantivy::Index;
use serde_json::json;

pub fn run_search_cli(matches: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(matches.get_one::<String>("index").unwrap());
    let query = matches.get_one::<String>("query").unwrap();
    let limit: usize = *matches.get_one::<usize>("limit").unwrap_or(&20usize);
    let offset: usize = *matches.get_one::<usize>("offset").unwrap_or(&0usize);
    let sort_by = matches.get_one::<String>("sort_by");
    let sort_desc = matches.contains_id("sort_desc");
    let agg = matches.get_one::<String>("aggregation");
    run_search(&index_directory, query, limit, offset, sort_by, sort_desc, &agg)
        .map_err(|e| format!("{:?}", e))
}

fn run_search(
    directory: &Path,
    query: &str,
    limit: usize,
    offset: usize,
    sort_by: Option<&String>,
    sort_desc: bool,
    agg: &std::option::Option<&String>,
) -> tantivy::Result<()> {
    let index = super::open_index(directory)?;
    let schema = index.schema();
    let default_fields: Vec<Field> = schema
        .fields()
        .filter(|(_, field_entry)| match *field_entry.field_type() {
            FieldType::Str(ref text_field_options) => {
                text_field_options.get_indexing_options().is_some()
            }
            _ => false,
        })
        .map(|(field, _)| field)
        .collect();
    let query_parser = QueryParser::new(schema.clone(), default_fields, index.tokenizers().clone());
    let query = query_parser.parse_query(query)?;
    let searcher = index.reader()?.searcher();

    let mut stdout = io::BufWriter::new(io::stdout());
    if let Some(agg) = agg {
        let agg_req: Aggregations = serde_json::from_str(agg).unwrap();
        let collector = AggregationCollector::from_aggs(agg_req, AggregationLimitsGuard::default());
        let agg_res = searcher.search(&query, &collector).unwrap();

        if let Err(e) = writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&agg_res).unwrap()
        ) {
            if e.kind() != ErrorKind::BrokenPipe {
                eprintln!("{}", e);
                process::exit(1)
            }
        }
    } else {
        let enable_scoring = EnableScoring::enabled_from_searcher(&searcher);
        let order = if sort_desc { Order::Desc } else { Order::Asc };

        if let Some(sort_field) = sort_by {
            // Sort by a fast field. We keep the u64 monotonic representation for JSON output.
            let collector = TopDocs::with_limit(limit)
                .and_offset(offset)
                .order_by_fast_field(sort_field.as_str(), order);
            let top_docs: Vec<(u64, tantivy::DocAddress)> = searcher.search(&query, &collector)?;

            for (sort_val, doc_address) in top_docs {
                let doc: TantivyDocument = searcher.doc(doc_address)?;
                let named_doc: NamedFieldDocument = doc.to_named_doc(&schema);
                let out = json!({
                    "sort": sort_val,
                    "doc": named_doc,
                });
                if let Err(e) = writeln!(stdout, "{}", serde_json::to_string(&out).unwrap()) {
                    if e.kind() != ErrorKind::BrokenPipe {
                        eprintln!("{}", e);
                        process::exit(1)
                    }
                }
            }
        } else {
            let collector = (TopDocs::with_limit(limit).and_offset(offset), Count);
            let (top_docs, _count): (Vec<(f32, tantivy::DocAddress)>, usize) =
                searcher.search(&query, &collector)?;
            for (score, doc_address) in top_docs {
                let doc: TantivyDocument = searcher.doc(doc_address)?;
                let named_doc: NamedFieldDocument = doc.to_named_doc(&schema);
                let out = json!({
                    "score": score,
                    "doc": named_doc,
                });
                if let Err(e) = writeln!(stdout, "{}", serde_json::to_string(&out).unwrap()) {
                    if e.kind() != ErrorKind::BrokenPipe {
                        eprintln!("{}", e);
                        process::exit(1)
                    }
                }
            }
        }
        let _ = enable_scoring;
    }
    stdout.flush()?;

    Ok(())
}

use clap::ArgMatches;
use std::convert::From;
use std::path::PathBuf;
use tantivy::schema::FieldType;
use tantivy::TantivyDocument;
use tantivy::Term;

pub fn run_delete_cli(matches: &ArgMatches) -> Result<(), String> {
    let index_directory = PathBuf::from(matches.get_one::<String>("index").unwrap());
    let field_name = matches
        .get_one::<String>("field")
        .map(|s| s.as_str())
        .unwrap_or("id");
    let value = matches.get_one::<String>("value").unwrap();

    run_delete(&index_directory, field_name, value).map_err(From::from)
}

fn run_delete(directory: &PathBuf, field_name: &str, value: &str) -> Result<(), String> {
    let index = super::open_index(directory).map_err(|e| format!("Failed to open index: {:?}", e))?;
    let schema = index.schema();

    let field = schema
        .get_field(field_name)
        .map_err(|_| format!("Unknown field: {}", field_name))?;

    // Currently we only support deleting via string terms (most common for id).
    // Tantivy requires the field type to be Str for Term::from_field_text.
    let entry = schema.get_field_entry(field);
    match entry.field_type() {
        FieldType::Str(_) => {
            let term = Term::from_field_text(field, value);
            let mut writer = index
                .writer::<TantivyDocument>(50_000_000)
                .map_err(|e| format!("Failed to create writer: {:?}", e))?;
            writer.delete_term(term);
            writer
                .commit()
                .map_err(|e| format!("Commit failed: {:?}", e))?;
            Ok(())
        }
        _ => Err(format!(
            "Field {} is not a string field; delete currently supports only string fields",
            field_name
        )),
    }
}

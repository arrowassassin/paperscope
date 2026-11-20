use crate::cli::{Workspace, EXIT_ERROR, EXIT_FOUND, EXIT_NOT_FOUND};
use anyhow::{Context, Result};
use clap::Args;
use rusqlite::OptionalExtension;

#[derive(Args)]
pub struct AggregateArgs {
    /// Field to aggregate: "date" or "amount"
    #[arg(long, value_parser = ["date", "amount"])]
    pub field: String,

    /// Operation: min, max, count (for date); sum, avg, count (for amount)
    #[arg(long, value_parser = ["min", "max", "count", "sum", "avg"])]
    pub op: String,

    /// Filter expression: kind=warranty|receipt|invoice|contract
    #[arg(long)]
    pub filter: Option<String>,

    /// Only files modified on or after this date (YYYY-MM-DD)
    #[arg(long)]
    pub after: Option<String>,

    /// Only files modified on or before this date (YYYY-MM-DD)
    #[arg(long)]
    pub before: Option<String>,

    /// Only these extensions, comma-separated (pdf,docx)
    #[arg(long, value_delimiter = ',')]
    pub ext: Vec<String>,

    /// Only paths containing this substring
    #[arg(long)]
    pub path: Option<String>,

    /// Search roots (overrides config)
    #[arg(long, short)]
    pub root: Vec<String>,
}

pub async fn run(args: AggregateArgs) -> Result<i32> {
    let ws = Workspace::new();
    let conn = ws.open_index()?;

    let roots = ws.roots(&args.root);
    let root_paths: Vec<String> = roots.iter().map(|p| p.to_string_lossy().to_string()).collect();

    // Build WHERE clause for source_path filtering
    let mut where_clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    // Root filtering: only chunks from files under the specified roots
    if !root_paths.is_empty() {
        let placeholders = root_paths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        where_clauses.push(format!("source_path IN ({placeholders})"));
        for rp in &root_paths {
            params.push(Box::new(rp.clone()));
        }
    }

    // Filter by kind
    if let Some(filter) = &args.filter {
        if filter.starts_with("kind=") {
            let kind = &filter[5..];
            match kind {
                "warranty" | "receipt" | "invoice" | "contract" => {
                    where_clauses.push("json_extract(metadata, '$.kind') = ?".to_string());
                    params.push(Box::new(kind.to_string()));
                }
                _ => {
                    eprintln!("invalid kind filter: {kind} (must be warranty, receipt, invoice, or contract)");
                    return Ok(EXIT_ERROR);
                }
            }
        } else {
            eprintln!("invalid filter format: {filter} (expected kind=...)");
            return Ok(EXIT_ERROR);
        }
    }

    // Date range filters on mtime
    if let Some(after) = &args.after {
        where_clauses.push("mtime >= ?".to_string());
        params.push(Box::new(parse_date_to_millis(after)?));
    }
    if let Some(before) = &args.before {
        where_clauses.push("mtime <= ?".to_string());
        params.push(Box::new(parse_date_to_millis(before)?));
    }

    // Extension filter
    if !args.ext.is_empty() {
        let _placeholders = args.ext.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        where_clauses.push(format!("path LIKE ?"));
        for ext in &args.ext {
            params.push(Box::new(format!("%.{ext}")));
        }
    }

    // Path substring filter
    if let Some(path_substr) = &args.path {
        where_clauses.push("path LIKE ?".to_string());
        params.push(Box::new(format!("%{path_substr}%")));
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // Build the query based on field and operation
    let (query, output_format) = match (args.field.as_str(), args.op.as_str()) {
        ("date", "min") => (
            format!(
                "SELECT MIN(json_extract(metadata, '$.dates[0]')) FROM chunks {where_sql}"
            ),
            "date",
        ),
        ("date", "max") => (
            format!(
                "SELECT MAX(json_extract(metadata, '$.dates[0]')) FROM chunks {where_sql}"
            ),
            "date",
        ),
        ("date", "count") => (
            format!(
                "SELECT COUNT(*) FROM chunks {where_sql} AND json_type(metadata, '$.dates') = 'array' AND json_array_length(metadata, '$.dates') > 0"
            ),
            "count",
        ),
        ("amount", "sum") => (
            format!(
                "SELECT SUM(json_extract(value, '$.amount')) FROM chunks {where_sql}, json_each(metadata, '$.amounts')"
            ),
            "amount",
        ),
        ("amount", "avg") => (
            format!(
                "SELECT AVG(json_extract(value, '$.amount')) FROM chunks {where_sql}, json_each(metadata, '$.amounts')"
            ),
            "amount",
        ),
        ("amount", "count") => (
            format!(
                "SELECT COUNT(*) FROM chunks {where_sql} AND json_type(metadata, '$.amounts') = 'array' AND json_array_length(metadata, '$.amounts') > 0"
            ),
            "count",
        ),
        _ => {
            eprintln!("invalid field/op combination: {}/{}", args.field, args.op);
            return Ok(EXIT_ERROR);
        }
    };

    // Convert params to slice for query_row
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let result: Option<String> = conn
        .inner()
        .query_row(&query, rusqlite::params_from_iter(param_refs.iter()), |row| {
            row.get(0)
        })
        .optional()?;

    match result {
        Some(val) => {
            match output_format {
                "date" => println!("{}", val),
                "amount" => {
                    if let Ok(num) = val.parse::<f64>() {
                        println!("{:.2}", num);
                    } else {
                        println!("{}", val);
                    }
                }
                "count" => println!("{}", val),
                _ => println!("{}", val),
            }
            Ok(EXIT_FOUND)
        }
        None => {
            eprintln!("no matching chunks");
            Ok(EXIT_NOT_FOUND)
        }
    }
}

fn parse_date_to_millis(date: &str) -> Result<i64> {
    // Parse YYYY-MM-DD to milliseconds since epoch
    let naive = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .with_context(|| format!("invalid date format: {date} (expected YYYY-MM-DD)"))?;
    let datetime = naive.and_hms_opt(0, 0, 0).unwrap();
    let timestamp = datetime.and_utc().timestamp_millis();
    Ok(timestamp)
}
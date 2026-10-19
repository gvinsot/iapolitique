//! Static audit of the sources (AGENTS.md §14.1), so that a future change cannot silently
//! reintroduce what the security review removed:
//!
//! - **SQL**: sqlx 0.9 only accepts `&'static str` queries (`SqlSafeStr`); a query built at run
//!   time needs `AssertSqlSafe`, `SqlStr`, `QueryBuilder` or `raw_sql`. None of them may appear:
//!   every query is a literal or a `concat!` of literals, and the dynamic parts (sort, filters)
//!   are closed Rust `enum`s mapped to constant fragments, values always `bind`.
//! - **Inputs**: every struct deserialized from a request (`src/http/`) refuses unknown fields
//!   (`#[serde(deny_unknown_fields)]`), except the documented read-only shapes.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

fn sources(subdir: &str) -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(subdir);
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.sort();
    files
        .into_iter()
        .filter(|path| !path.ends_with("source_audit_tests.rs"))
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect()
}

#[test]
fn sql_is_never_built_at_run_time() {
    let forbidden = ["AssertSqlSafe", "SqlStr", "QueryBuilder", "raw_sql"];
    let mut found = Vec::new();
    for (path, text) in sources("") {
        for (number, line) in text.lines().enumerate() {
            if forbidden.iter().any(|word| line.contains(word)) {
                found.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(found.is_empty(), "dynamic SQL:\n{}", found.join("\n"));
}

/// Shapes deserialized from the database or from our own spec, never from a request.
const NOT_REQUEST_INPUTS: &[&str] = &[
    "ErrorBody",
    "ErrorDetail",
    "SourceReference",
    "StoredAnalysis",
    "StoredEstimate",
    "StoredFeasibility",
    "StoredImpact",
];

#[test]
fn request_structs_refuse_unknown_fields() {
    let mut missing = Vec::new();
    let mut checked = 0;
    for (path, text) in sources("http") {
        let name = path.file_name().unwrap().to_string_lossy();
        if name.ends_with("_tests.rs") {
            continue;
        }
        // Production part of the file only (unit tests may declare their own payloads).
        let text = text.split("#[cfg(test)]\nmod tests").next().unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let line = line.trim();
            if !(line.starts_with("#[derive(") && line.contains("Deserialize")) {
                continue;
            }
            // Attributes up to the item declaration.
            let mut attributes = Vec::new();
            let mut item = "";
            for next in &lines[index + 1..] {
                let next = next.trim();
                if next.starts_with("#[") || next.starts_with("///") {
                    attributes.push(next);
                } else {
                    item = next;
                    break;
                }
            }
            if !item.contains("struct ") {
                continue; // closed enums
            }
            let struct_name = item
                .split("struct ")
                .nth(1)
                .and_then(|rest| rest.split([' ', '<', '{', '(']).next())
                .unwrap_or_default();
            if NOT_REQUEST_INPUTS.contains(&struct_name) {
                continue;
            }
            checked += 1;
            if !attributes
                .iter()
                .any(|attribute| attribute.contains("deny_unknown_fields"))
            {
                missing.push(format!("{}: {struct_name}", path.display()));
            }
        }
    }
    assert!(checked >= 20, "only {checked} request structs found");
    assert!(
        missing.is_empty(),
        "without deny_unknown_fields:\n{}",
        missing.join("\n")
    );
}

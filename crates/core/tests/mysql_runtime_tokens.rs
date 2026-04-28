use std::fs;
use std::path::{Path, PathBuf};

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read_dir should succeed") {
        let entry = entry.expect("dir entry should succeed");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

#[test]
fn core_source_does_not_reference_mysql_runtime_tokens() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut rust_files = Vec::new();
    collect_rust_files(&src_dir, &mut rust_files);

    let forbidden_tokens = [
        concat!("rbdc_", "mysql"),
        concat!("Mysql", "Driver"),
        concat!("sqlx::", "My", "Sql"),
        concat!("My", "Sql", "Pool"),
        concat!("Pool<", "My", "Sql>"),
        concat!("DB", "_HOST"),
        concat!("my", "sql://"),
    ];

    let mut violations = Vec::new();
    for file in rust_files {
        let content = fs::read_to_string(&file).expect("source file should be readable");
        for token in forbidden_tokens {
            if content.contains(token) {
                violations.push(format!("{} -> {}", file.display(), token));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "core source still contains MySQL runtime tokens:\n{}",
        violations.join("\n")
    );
}

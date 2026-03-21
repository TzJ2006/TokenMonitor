// src-tauri/src/change_stats.rs

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FileCategory {
    Code,
    Docs,
    Config,
    Other,
}

pub fn classify_file(path: &str) -> FileCategory {
    let ext = match path.rsplit('.').next() {
        Some(e) => e.to_ascii_lowercase(),
        None => return FileCategory::Other,
    };

    match ext.as_str() {
        // Code
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py" | "go" | "java" | "kt"
        | "scala" | "swift" | "c" | "cc" | "cpp" | "h" | "hpp" | "cs" | "rb" | "php" | "sh"
        | "bash" | "zsh" | "sql" | "html" | "css" | "scss" | "sass" | "svelte" | "vue" => {
            FileCategory::Code
        }

        // Docs
        "md" | "mdx" | "txt" | "rst" | "adoc" | "asciidoc" => FileCategory::Docs,

        // Config
        "json" | "yaml" | "yml" | "toml" | "ini" | "env" | "xml" => FileCategory::Config,

        _ => FileCategory::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_rust_file() {
        assert_eq!(classify_file("src/main.rs"), FileCategory::Code);
    }

    #[test]
    fn classify_typescript_file() {
        assert_eq!(classify_file("src/lib/types/index.ts"), FileCategory::Code);
    }

    #[test]
    fn classify_svelte_file() {
        assert_eq!(classify_file("src/App.svelte"), FileCategory::Code);
    }

    #[test]
    fn classify_markdown_file() {
        assert_eq!(classify_file("docs/README.md"), FileCategory::Docs);
    }

    #[test]
    fn classify_json_file() {
        assert_eq!(classify_file("package.json"), FileCategory::Config);
    }

    #[test]
    fn classify_yaml_file() {
        assert_eq!(
            classify_file(".github/workflows/ci.yml"),
            FileCategory::Config
        );
    }

    #[test]
    fn classify_unknown_extension() {
        assert_eq!(classify_file("image.png"), FileCategory::Other);
    }

    #[test]
    fn classify_no_extension() {
        assert_eq!(classify_file("Makefile"), FileCategory::Other);
    }

    #[test]
    fn classify_case_insensitive() {
        assert_eq!(classify_file("README.MD"), FileCategory::Docs);
    }
}

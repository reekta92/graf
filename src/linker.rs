use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use glob::{Pattern, glob};

#[derive(Debug, Clone)]
pub struct FileData {
    pub relative_path: String,
    pub title: String,
    pub tags: Vec<String>,
    pub wikilinks: Vec<String>,
}

pub fn scan_markdown_files(
    base_dir: &Path,
    exclude_patterns: &[String],
    max_nodes: usize,
) -> Vec<FileData> {
    let mut files: Vec<_> = markdown_paths(base_dir, exclude_patterns)
        .into_iter()
        .filter_map(|path| parse_markdown_file(&path, base_dir).ok())
        .collect();
    if max_nodes > 0 {
        files.truncate(max_nodes);
    }
    files
}

pub(crate) fn markdown_paths(
    base_dir: &Path,
    exclude_patterns: &[String],
) -> Vec<std::path::PathBuf> {
    let excluded: Vec<_> = exclude_patterns
        .iter()
        .filter_map(|p| Pattern::new(p.trim_end_matches('/')).ok())
        .collect();
    let mut files = Vec::new();

    let patterns = ["**/*.md", "**/*.mdx"];

    for pattern in &patterns {
        let full_pattern = format!("{}/{pattern}", Pattern::escape(&base_dir.to_string_lossy()));
        if let Ok(paths) = glob(&full_pattern) {
            for entry in paths.flatten() {
                if should_exclude(&entry, base_dir, &excluded) {
                    continue;
                }
                files.push(entry);
            }
        }
    }

    files.sort();
    files
}

fn should_exclude(path: &Path, base: &Path, patterns: &[Pattern]) -> bool {
    path.strip_prefix(base).is_ok_and(|rel| {
        rel.ancestors()
            .any(|ancestor| patterns.iter().any(|p| p.matches_path(ancestor)))
    })
}

fn parse_markdown_file(path: &Path, base: &Path) -> anyhow::Result<FileData> {
    let content = fs::read_to_string(path)?;
    let relative_path = path
        .strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");

    Ok(crate::document::parse_note(&content, &relative_path, &Default::default())?.data)
}

pub fn resolve_links(files: &[FileData], exclude_tags: &[String]) -> HashMap<String, Vec<String>> {
    let title_to_path: HashMap<String, String> = files
        .iter()
        .filter(|f| exclude_tags.is_empty() || f.tags.iter().all(|t| !exclude_tags.contains(t)))
        .map(|f| (f.title.to_lowercase(), f.relative_path.clone()))
        .collect();

    let mut links: HashMap<String, Vec<String>> = HashMap::new();

    for file in files {
        if exclude_tags.iter().any(|t| file.tags.contains(t)) {
            continue;
        }
        let mut seen = HashSet::new();
        let mut targets = Vec::new();
        for link in &file.wikilinks {
            if let Some(target) = title_to_path.get(&link.to_lowercase())
                && target != &file.relative_path
                && seen.insert(target.clone())
            {
                targets.push(target.clone());
            }
        }
        links.insert(file.relative_path.clone(), targets);
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::test_support::Notebook as TestDir;

    #[test]
    fn scans_literal_root_and_excludes_directory_descendants() {
        let temp = TestDir::new();
        let root = temp.path().join("[notes]");
        fs::create_dir_all(root.join("archive/deep")).unwrap();
        fs::create_dir_all(root.join("private")).unwrap();
        for path in [
            "archive/deep/hidden.md",
            "private/hidden.md",
            "keep.md",
            "first.mdx",
        ] {
            fs::write(root.join(path), "# Note\n").unwrap();
        }
        let files = scan_markdown_files(&root, &["archive/*".into(), "private/".into()], 0);
        assert_eq!(
            files
                .iter()
                .map(|f| f.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["first.mdx", "keep.md"]
        );
        assert_eq!(
            scan_markdown_files(&root, &[], 1)[0].relative_path,
            "archive/deep/hidden.md"
        );
    }

    #[test]
    fn unquoted_titles_preserve_spaces_and_ignore_comments() {
        let note = crate::document::parse_note(
            "---\ntitle: My Note # comment\n---\n",
            "a.md",
            &Default::default(),
        )
        .unwrap();
        assert_eq!(note.data.title, "My Note");
    }
}

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use glob::glob;

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
    let excluded = resolve_exclude_set(base_dir, exclude_patterns);
    let mut files = Vec::new();

    let patterns = ["**/*.md", "**/*.mdx"];

    for pattern in &patterns {
        let full_pattern = base_dir.join(pattern).to_string_lossy().to_string();
        if let Ok(paths) = glob(&full_pattern) {
            for entry in paths.flatten() {
                if should_exclude(&entry, base_dir, exclude_patterns, &excluded) {
                    continue;
                }
                files.push(entry);
            }
        }
    }

    files.sort();
    files
}

fn resolve_exclude_set(base_dir: &Path, patterns: &[String]) -> HashSet<std::path::PathBuf> {
    let mut excluded = HashSet::new();
    for pat in patterns {
        if let Ok(paths) = glob(&base_dir.join(pat).to_string_lossy()) {
            for path in paths.flatten() {
                excluded.insert(path);
            }
        }
    }
    excluded
}

fn should_exclude(
    path: &Path,
    base: &Path,
    patterns: &[String],
    excluded: &HashSet<std::path::PathBuf>,
) -> bool {
    if excluded.contains(path) {
        return true;
    }
    if let Ok(rel) = path.strip_prefix(base) {
        let rel_str = rel.to_string_lossy();
        for pat in patterns {
            if rel_str.contains(pat) || rel_str.ends_with(pat) {
                return true;
            }
        }
    }
    false
}

fn parse_markdown_file(path: &Path, base: &Path) -> anyhow::Result<FileData> {
    let content = fs::read_to_string(path)?;
    let relative_path = path
        .strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

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

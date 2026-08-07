use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use glob::glob;
use regex::Regex;
use std::sync::LazyLock;

static WIKILINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap());
static FRONTMATTER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n").unwrap());
static TAGS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^tags:\s*\[([^\]]*)\]").unwrap());
static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^title:\s*(?:"([^"]*)"|'([^']*)'|([^\s#]+))"#).unwrap());

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
                if let Ok(data) = parse_markdown_file(&entry, base_dir) {
                    files.push(data);
                }
            }
        }
    }

    if max_nodes > 0 && files.len() > max_nodes {
        files.truncate(max_nodes);
    }

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
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

    let tags = extract_tags(&content);
    let wikilinks = extract_wikilinks(&content);
    let title = extract_title(&content).unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| relative_path.clone())
    });

    Ok(FileData {
        relative_path,
        title,
        tags,
        wikilinks,
    })
}

fn extract_wikilinks(content: &str) -> Vec<String> {
    WIKILINK_RE
        .captures_iter(content)
        .filter_map(|c| c.get(1).map(|m| m.as_str().trim().to_string()))
        .collect()
}

fn extract_tags(content: &str) -> Vec<String> {
    if let Some(fm) = FRONTMATTER_RE.captures(content) {
        let fm_content = fm.get(1).unwrap().as_str();
        if let Some(tags_match) = TAGS_RE.captures(fm_content)
            && let Some(tags_str) = tags_match.get(1)
        {
            return tags_str
                .as_str()
                .split(',')
                .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
                .filter(|t| !t.is_empty())
                .collect();
        }
    }
    Vec::new()
}

fn extract_title(content: &str) -> Option<String> {
    if let Some(fm) = FRONTMATTER_RE.captures(content) {
        let fm_content = fm.get(1).unwrap().as_str();
        if let Some(title_match) = TITLE_RE.captures(fm_content) {
            if let Some(m) = title_match.get(1) {
                return Some(m.as_str().to_string());
            }
            if let Some(m) = title_match.get(2) {
                return Some(m.as_str().to_string());
            }
            if let Some(m) = title_match.get(3) {
                return Some(m.as_str().to_string());
            }
        }
    }

    content
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l[2..].trim().to_string())
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

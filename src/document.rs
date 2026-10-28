//! Markdown parsing and file-based link resolution for the standalone host.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

use crate::linker::FileData;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkResolution {
    #[default]
    TitleFirst,
    FilenameFirst,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkFormat {
    #[default]
    Wiki,
    Markdown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ParsingConfig {
    pub link_resolution: LinkResolution,
    pub link_format: LinkFormat,
    pub hashtags: bool,
    pub colon_tags: bool,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub data: FileData,
    pub links: Vec<NoteLink>,
}

#[derive(Debug, Clone)]
pub struct NoteLink {
    pub destination: String,
    pub wiki: bool,
    /// The complete link occurrence, including its label, for precise removal.
    pub range: Range<usize>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Metadata {
    title: Option<String>,
    tags: Vec<String>,
    keywords: Vec<String>,
}

fn frontmatter(content: &str) -> Result<(Metadata, usize)> {
    let mut lines = content.split_inclusive('\n');
    if !lines.next().is_some_and(|l| l.trim_end() == "---") {
        return Ok((Metadata::default(), 0));
    }
    let start = content.find('\n').map_or(content.len(), |i| i + 1);
    let mut end = start;
    for line in lines {
        if matches!(line.trim_end(), "---" | "...") {
            let metadata = serde_saphyr::from_str::<Option<Metadata>>(&content[start..end])
                .context("invalid YAML frontmatter")?
                .unwrap_or_default();
            return Ok((metadata, end + line.len()));
        }
        end += line.len();
    }
    // An unclosed delimiter is ordinary Markdown (for example, a thematic break).
    Ok((Metadata::default(), 0))
}

pub fn parse_note(content: &str, relative_path: &str, config: &ParsingConfig) -> Result<Note> {
    let (metadata, offset) = frontmatter(content)?;
    let mut title = metadata.title.filter(|t| !t.trim().is_empty());
    let mut tags = metadata.tags;
    tags.extend(metadata.keywords);
    let mut links = Vec::new();
    let mut heading = None;
    let mut in_code = false;
    let mut link_depth = 0usize;
    for (event, range) in
        Parser::new_ext(&content[offset..], Options::ENABLE_WIKILINKS).into_offset_iter()
    {
        let range = range.start + offset..range.end + offset;
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_code = true,
            Event::End(TagEnd::CodeBlock) => in_code = false,
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) if title.is_none() => heading = Some(String::new()),
            Event::End(TagEnd::Heading(HeadingLevel::H1)) => {
                if let Some(text) = heading.take().filter(|s| !s.trim().is_empty()) {
                    title = Some(text.trim().to_string());
                }
            }
            Event::Start(
                Tag::Link {
                    dest_url,
                    link_type,
                    ..
                }
                | Tag::Image {
                    dest_url,
                    link_type,
                    ..
                },
            ) => {
                links.push(NoteLink {
                    destination: dest_url.into_string(),
                    wiki: matches!(link_type, LinkType::WikiLink { .. }),
                    range,
                });
                link_depth += 1;
            }
            Event::End(TagEnd::Link | TagEnd::Image) => link_depth = link_depth.saturating_sub(1),
            Event::Text(text) | Event::Code(text) if heading.is_some() => {
                heading.as_mut().unwrap().push_str(&text)
            }
            Event::Text(_) if !in_code && link_depth == 0 => {
                extract_inline_tags(content, range, config, &mut tags)
            }
            _ => {}
        }
    }
    let mut seen = HashSet::new();
    tags.retain(|t| !t.is_empty() && seen.insert(t.to_lowercase()));
    let title = title.unwrap_or_else(|| {
        Path::new(relative_path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    });
    let wikilinks = links
        .iter()
        .filter(|l| l.wiki)
        .map(|l| l.destination.clone())
        .collect();
    Ok(Note {
        data: FileData {
            relative_path: relative_path.into(),
            title,
            tags,
            wikilinks,
        },
        links,
    })
}

fn tag_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/') || c as u32 >= 0x1f000
}

fn extract_inline_tags(
    content: &str,
    range: Range<usize>,
    config: &ParsingConfig,
    tags: &mut Vec<String>,
) {
    for (index, marker) in content[range.clone()].char_indices() {
        if !(marker == '#' && config.hashtags || marker == ':' && config.colon_tags) {
            continue;
        }
        let start = range.start + index;
        if content[..start]
            .chars()
            .last()
            .is_some_and(|c| tag_char(c) || matches!(c, '\\' | '/' | ':'))
        {
            continue;
        }
        let mut rest = &content[start + 1..range.end];
        loop {
            let end = rest.find(|c: char| !tag_char(c)).unwrap_or(rest.len());
            let tag = &rest[..end];
            if tag.is_empty()
                || !tag.chars().any(|c| !c.is_numeric())
                || marker == ':' && !rest[end..].starts_with(':')
            {
                break;
            }
            tags.push(tag.to_string());
            if marker == '#' {
                break;
            }
            rest = &rest[end + 1..];
        }
    }
}

pub fn scan_notes(
    base: &Path,
    exclude: &[String],
    config: &ParsingConfig,
) -> (Vec<Note>, Vec<String>) {
    let mut notes = Vec::new();
    let mut errors = Vec::new();
    for path in crate::linker::markdown_paths(base, exclude) {
        let relative = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let result = std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|content| parse_note(&content, &relative, config));
        match result {
            Ok(note) => notes.push(note),
            Err(e) => errors.push(format!("{relative}: {e:#}")),
        }
    }
    (notes, errors)
}

/// Index the full notebook before applying visibility filters or node limits.
pub struct LinkResolver<'a> {
    paths: HashSet<&'a str>,
    stems: HashMap<String, Vec<&'a str>>,
    suffixes: HashMap<String, Vec<&'a str>>,
    titles: HashMap<String, Vec<&'a str>>,
    mode: LinkResolution,
}

impl<'a> LinkResolver<'a> {
    pub fn new(notes: &'a [Note], mode: LinkResolution) -> Self {
        let mut resolver = Self {
            paths: HashSet::new(),
            stems: HashMap::new(),
            suffixes: HashMap::new(),
            titles: HashMap::new(),
            mode,
        };
        for note in notes {
            let data = &note.data;
            resolver.paths.insert(&data.relative_path);
            for (index, _) in data.relative_path.match_indices('/') {
                let suffix = &data.relative_path[index + 1..];
                resolver
                    .suffixes
                    .entry(suffix.to_string())
                    .or_default()
                    .push(&data.relative_path);
                resolver
                    .suffixes
                    .entry(strip_extension(suffix).to_string())
                    .or_default()
                    .push(&data.relative_path);
            }
            let stem = Path::new(&data.relative_path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            resolver
                .stems
                .entry(stem)
                .or_default()
                .push(&data.relative_path);
            resolver
                .titles
                .entry(data.title.to_lowercase())
                .or_default()
                .push(&data.relative_path);
        }
        resolver
    }

    pub fn resolve(&self, source: &str, link: &NoteLink) -> Result<Option<&'a str>> {
        let raw = link.destination.trim();
        let scheme = raw.split_once(':').is_some_and(|(prefix, _)| {
            prefix.starts_with(|c: char| c.is_ascii_alphabetic())
                && prefix
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
        if raw.contains("://") || raw.starts_with("//") || !link.wiki && scheme {
            return Ok(None);
        }
        let raw = raw.split('#').next().unwrap_or(raw);
        let decoded = if link.wiki {
            raw.into()
        } else {
            percent_decode_str(raw)
                .decode_utf8()
                .context("invalid UTF-8 in link")?
        };
        let target = decoded.as_ref();
        if target.is_empty() {
            return Ok(None);
        }
        let explicit = target.contains('/') || target.ends_with(".md") || target.ends_with(".mdx");
        if link.wiki
            && !explicit
            && self.mode == LinkResolution::TitleFirst
            && let Some(found) = unique(self.titles.get(&target.to_lowercase()), target)?
        {
            return Ok(Some(found));
        }
        let parent = Path::new(source).parent().unwrap_or_else(|| Path::new(""));
        let source_path = normalize(&parent.join(target));
        let root_path = normalize(Path::new(target.trim_start_matches('/')));
        let mut candidates = Vec::new();
        if !target.starts_with('/') && (!link.wiki || target.starts_with('.')) {
            candidates.extend(source_path);
        }
        if link.wiki {
            candidates.extend(root_path);
        }
        for path in candidates {
            if let Some(found) = self.paths.get(path.as_str()) {
                return Ok(Some(found));
            }
            let variants = [format!("{path}.md"), format!("{path}.mdx")];
            let matches: Vec<_> = variants
                .iter()
                .filter_map(|p| self.paths.get(p.as_str()).copied())
                .collect();
            if let Some(found) = unique(Some(&matches), target)? {
                return Ok(Some(found));
            }
        }
        if link.wiki && !target.starts_with('.') && !target.starts_with('/') {
            if explicit && let Some(found) = unique(self.suffixes.get(target), target)? {
                return Ok(Some(found));
            }
            if !explicit
                && let Some(found) = unique(self.stems.get(&target.to_lowercase()), target)?
            {
                return Ok(Some(found));
            }
            if !explicit && self.mode == LinkResolution::FilenameFirst {
                return unique(self.titles.get(&target.to_lowercase()), target);
            }
        }
        Ok(None)
    }
}

fn unique<'a>(matches: Option<&Vec<&'a str>>, target: &str) -> Result<Option<&'a str>> {
    match matches.map(Vec::as_slice).unwrap_or_default() {
        [] => Ok(None),
        [path] => Ok(Some(path)),
        _ => bail!("ambiguous link {target:?}; use an explicit path"),
    }
}

fn normalize(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.components() {
        match part {
            Component::Normal(p) => parts.push(p.to_string_lossy()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

fn strip_extension(path: &str) -> &str {
    path.strip_suffix(".mdx")
        .or_else(|| path.strip_suffix(".md"))
        .unwrap_or(path)
}

pub fn resolve_note_links(
    notes: &[Note],
    mode: LinkResolution,
) -> (HashMap<String, Vec<String>>, Vec<String>) {
    let resolver = LinkResolver::new(notes, mode);
    let mut edges = HashMap::new();
    let mut errors = Vec::new();
    for note in notes {
        let mut targets = HashSet::new();
        for link in &note.links {
            match resolver.resolve(&note.data.relative_path, link) {
                Ok(Some(path)) if path != note.data.relative_path => {
                    targets.insert(path.to_string());
                }
                Err(e) => errors.push(format!("{}: {e}", note.data.relative_path)),
                _ => {}
            }
        }
        let mut targets: Vec<_> = targets.into_iter().collect();
        targets.sort();
        edges.insert(note.data.relative_path.clone(), targets);
    }
    (edges, errors)
}

/// Edit references by resolved identity, leaving unrelated markup verbatim.
pub fn edit_connection(
    content: &str,
    source: &str,
    target: &str,
    notes: &[Note],
    config: &ParsingConfig,
    create: bool,
) -> Result<String> {
    anyhow::ensure!(source != target, "cannot connect a note to itself");
    let resolver = LinkResolver::new(notes, config.link_resolution);
    let parsed = parse_note(content, source, config)?;
    let mut ranges = Vec::new();
    for link in &parsed.links {
        if resolver.resolve(source, link).ok().flatten() == Some(target) {
            ranges.push(link.range.clone());
        }
    }
    if create {
        if !ranges.is_empty() {
            return Ok(content.to_string());
        }
        let target_note = notes
            .iter()
            .find(|n| n.data.relative_path == target)
            .context("target note no longer exists")?;
        let title = &target_note.data.title;
        let link = match config.link_format {
            LinkFormat::Wiki => {
                let title_link = NoteLink {
                    destination: title.clone(),
                    wiki: true,
                    range: 0..0,
                };
                let safe = |s: &str| !s.contains(['#', '|', '[', ']', '\\', '\n', '\r', ':', '^']);
                if config.link_resolution == LinkResolution::TitleFirst
                    && safe(title)
                    && resolver.resolve(source, &title_link).ok().flatten() == Some(target)
                {
                    format!("[[{title}]]")
                } else {
                    anyhow::ensure!(
                        safe(target),
                        "filename cannot be represented as a wikilink; choose parsing.link_format = markdown"
                    );
                    if safe(title) {
                        format!("[[{target}|{title}]]")
                    } else {
                        format!("[[{target}]]")
                    }
                }
            }
            LinkFormat::Markdown => {
                let parent: Vec<_> = source.split('/').collect();
                let parent = &parent[..parent.len() - 1];
                let parts: Vec<_> = target.split('/').collect();
                let common = parent
                    .iter()
                    .zip(&parts)
                    .take_while(|(a, b)| a == b)
                    .count();
                let path = format!(
                    "{}{}",
                    "../".repeat(parent.len() - common),
                    parts[common..].join("/")
                );
                const PATH: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
                    .remove(b'/')
                    .remove(b'-')
                    .remove(b'_')
                    .remove(b'.')
                    .remove(b'~');
                let path = percent_encoding::utf8_percent_encode(&path, PATH);
                let mut label = String::new();
                for c in title.chars() {
                    if c.is_ascii_punctuation() {
                        label.push('\\');
                    }
                    label.push(c);
                }
                format!("[{label}]({path})")
            }
        };
        let updated = crate::wikilink::append_link(content, &link);
        // The generated syntax must round-trip through the same parser and resolver.
        let generated = parse_note(&updated, source, config)?;
        anyhow::ensure!(
            generated
                .links
                .iter()
                .any(|l| resolver.resolve(source, l).ok().flatten() == Some(target)),
            "generated link does not resolve to the selected note"
        );
        return Ok(updated);
    }
    // Images may be nested inside links; remove each overlapping occurrence once.
    ranges.sort_by_key(|range| (range.start, std::cmp::Reverse(range.end)));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut().filter(|r| range.start < r.end) {
            previous.end = previous.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    let mut updated = content.to_string();
    for range in merged.into_iter().rev() {
        updated.replace_range(range, "");
    }
    Ok(updated)
}

/// Replace a note atomically, refusing paths outside the notebook and stale edits.
pub fn save_note(base: &Path, id: &str, original: &str, updated: &str) -> Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let root = base.canonicalize()?;
    let path = root.join(id).canonicalize()?;
    anyhow::ensure!(
        path.starts_with(&root),
        "refusing to edit a note outside the notebook"
    );
    let permissions = std::fs::metadata(&path)?.permissions();
    anyhow::ensure!(!permissions.readonly(), "note is read-only");
    let temp = path.with_file_name(format!(
        ".graf-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .context("cannot create temporary note")?;
    let result = (|| -> Result<()> {
        file.write_all(updated.as_bytes())?;
        file.sync_all()?;
        file.set_permissions(permissions)?;
        anyhow::ensure!(
            std::fs::read_to_string(&path)? == original,
            "note changed on disk; refresh and retry"
        );
        std::fs::rename(&temp, &path).context("cannot replace note")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    pub struct Notebook(pub PathBuf);
    impl Notebook {
        pub fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("development/scratch")
                .join(format!(
                    "note-compat-{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        pub fn write(&self, id: &str, content: &str) {
            let path = self.0.join(id);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
    }
    impl Drop for Notebook {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, content: &str) -> Note {
        parse_note(content, path, &ParsingConfig::default()).unwrap()
    }

    fn link(destination: &str, wiki: bool) -> NoteLink {
        NoteLink {
            destination: destination.into(),
            wiki,
            range: 0..0,
        }
    }

    #[test]
    fn parses_yaml_titles_and_tags_without_requiring_frontmatter() {
        let parsed = note(
            "ahj2.md",
            "---\ntitle: A Note on Titles\ntags:\n  - work\n  - rust\nkeywords: [work, notes]\n---\n# Other heading\n[[ahj2|alias]]",
        );
        assert_eq!(parsed.data.title, "A Note on Titles");
        assert_eq!(parsed.data.tags, ["work", "rust", "notes"]);
        assert_eq!(parsed.data.wikilinks, ["ahj2"]);
        assert_eq!(
            note("id.md", "# **Human** title\n").data.title,
            "Human title"
        );
        assert_eq!(note("id.md", "Body").data.title, "id");
        assert!(
            parse_note(
                "---\ntags: [broken\n---\n",
                "id.md",
                &ParsingConfig::default()
            )
            .is_err()
        );
    }

    #[test]
    fn ignores_code_comments_and_escapes_but_reads_reference_links() {
        let content = "# Heading\n`[[inline]]`\n```md\n# Wrong\n[[fenced]]\n```\n    [[indented]]\n\n<!-- [[comment]] -->\n\\[[escaped]]\n[[real|alias]] [Read][ref]\n\n[ref]: other.md\n";
        let parsed = note("source.md", content);
        assert_eq!(
            parsed
                .links
                .iter()
                .map(|l| l.destination.as_str())
                .collect::<Vec<_>>(),
            ["real", "other.md"]
        );
        for link in parsed.links {
            let source = &content[link.range];
            assert!(
                source == "[[real|alias]]" || source == "[Read][ref]",
                "{source}"
            );
        }
    }

    #[test]
    fn inline_tags_are_opt_in_handle_chains_and_preserve_primary_tag() {
        let content = "---\ntags: [work]\n---\n# Heading\n#work #rust, #nested/tag #日本語 :one:two:three: `#code`\n\\#escaped https://example/#url word:bad: #1984\n<!-- #comment -->\n";
        let defaults = parse_note(content, "a.md", &ParsingConfig::default()).unwrap();
        assert_eq!(defaults.data.tags, ["work"]);
        let config = ParsingConfig {
            hashtags: true,
            colon_tags: true,
            ..Default::default()
        };
        let parsed = parse_note(content, "a.md", &config).unwrap();
        assert_eq!(
            parsed.data.tags,
            [
                "work",
                "rust",
                "nested/tag",
                "日本語",
                "one",
                "two",
                "three"
            ]
        );
    }

    #[test]
    fn resolves_zk_filename_separately_from_its_label() {
        let notes = [
            note("a.md", "# Source\n[[ahj2|A Note on Titles]]"),
            note("ahj2.md", "# A Note on Titles"),
        ];
        let (links, errors) = resolve_note_links(&notes, LinkResolution::FilenameFirst);
        assert!(errors.is_empty());
        assert_eq!(links["a.md"], ["ahj2.md"]);
    }

    #[test]
    fn title_precedence_is_explicit_and_duplicate_titles_are_ambiguous() {
        let notes = [
            note("todo.md", "# Other"),
            note("other.md", "# todo"),
            note("duplicate.md", "# Other"),
        ];
        let old = LinkResolver::new(&notes, LinkResolution::TitleFirst);
        let files = LinkResolver::new(&notes, LinkResolution::FilenameFirst);
        assert_eq!(
            old.resolve("source.md", &link("todo", true)).unwrap(),
            Some("other.md")
        );
        assert_eq!(
            files.resolve("source.md", &link("todo", true)).unwrap(),
            Some("todo.md")
        );
        assert!(old.resolve("source.md", &link("Other", true)).is_err());
        assert_eq!(
            old.resolve("source.md", &link("./todo.md", true)).unwrap(),
            Some("todo.md")
        );
    }

    #[test]
    fn colons_in_wikilink_titles_are_not_treated_as_external_urls() {
        let notes = [
            note("a.md", "# Research: Notes"),
            note("mailto:note.md", "# Mail"),
        ];
        for mode in [LinkResolution::TitleFirst, LinkResolution::FilenameFirst] {
            let resolver = LinkResolver::new(&notes, mode);
            assert_eq!(
                resolver
                    .resolve("source.md", &link("Research: Notes", true))
                    .unwrap(),
                Some("a.md")
            );
            assert_eq!(
                resolver
                    .resolve("source.md", &link("mailto:note.md", false))
                    .unwrap(),
                None
            );
        }
    }

    #[test]
    fn resolves_paths_extensions_fragments_and_encoded_characters() {
        let notes = [
            note("folder/My Note.md", "# Title"),
            note("folder/hash#name.mdx", "# Hash"),
            note("folder/a.md", "# A"),
        ];
        let resolver = LinkResolver::new(&notes, LinkResolution::FilenameFirst);
        for (destination, wiki, expected) in [
            ("My%20Note.md#heading", false, Some("folder/My Note.md")),
            ("../folder/My%20Note", false, Some("folder/My Note.md")),
            ("folder/My Note#^block", true, Some("folder/My Note.md")),
            ("hash%23name.mdx", false, Some("folder/hash#name.mdx")),
            ("#local", true, None),
            ("https://example.com/a.md", false, None),
            ("../../folder/a.md", false, None),
            ("photo.png", false, None),
        ] {
            assert_eq!(
                resolver
                    .resolve("folder/source.md", &link(destination, wiki))
                    .unwrap(),
                expected,
                "{destination}"
            );
        }
        assert!(
            resolver
                .resolve("source.md", &link("%FF.md", false))
                .is_err()
        );
    }

    #[test]
    fn ambiguous_short_names_do_not_choose_by_scan_order() {
        let notes = [note("x/a.md", "# one"), note("y/a.md", "# two")];
        let resolver = LinkResolver::new(&notes, LinkResolution::FilenameFirst);
        assert!(resolver.resolve("source.md", &link("a", true)).is_err());
        assert_eq!(
            resolver.resolve("source.md", &link("x/a", true)).unwrap(),
            Some("x/a.md")
        );
    }

    #[test]
    fn edits_roundtrip_both_formats_with_duplicate_titles() {
        let notes = [
            note("source.md", "# Source\nBody.\n"),
            note("a.md", "# todo"),
            note("b.md", "# todo"),
        ];
        for format in [LinkFormat::Wiki, LinkFormat::Markdown] {
            for mode in [LinkResolution::TitleFirst, LinkResolution::FilenameFirst] {
                let config = ParsingConfig {
                    link_format: format,
                    link_resolution: mode,
                    ..Default::default()
                };
                let content = "# Source\nBody.\n";
                let updated =
                    edit_connection(content, "source.md", "a.md", &notes, &config, true).unwrap();
                let parsed = parse_note(&updated, "source.md", &config).unwrap();
                let resolver = LinkResolver::new(&notes, mode);
                assert_eq!(parsed.links.len(), 1);
                assert_eq!(
                    resolver.resolve("source.md", &parsed.links[0]).unwrap(),
                    Some("a.md")
                );
                assert_eq!(
                    edit_connection(&updated, "source.md", "a.md", &notes, &config, true).unwrap(),
                    updated
                );
                let removed =
                    edit_connection(&updated, "source.md", "a.md", &notes, &config, false).unwrap();
                assert!(
                    parse_note(&removed, "source.md", &config)
                        .unwrap()
                        .links
                        .is_empty()
                );
                assert!(removed.starts_with(content));
            }
        }
    }

    #[test]
    fn deletion_preserves_unrelated_text_definitions_and_code_examples() {
        let notes = [note("a.md", "# A"), note("b.md", "# B")];
        let content =
            "Before [[a|alias]] and [[a#Section]] [reference][id].\n[[b]] `[[a]]`\n\n[id]: a.md\n";
        let removed = edit_connection(
            content,
            "source.md",
            "a.md",
            &notes,
            &ParsingConfig::default(),
            false,
        )
        .unwrap();
        assert_eq!(removed, "Before  and  .\n[[b]] `[[a]]`\n\n[id]: a.md\n");
        let nested = "Before [![alt](a.md)](a.md) after.";
        assert_eq!(
            edit_connection(
                nested,
                "source.md",
                "a.md",
                &notes,
                &ParsingConfig::default(),
                false
            )
            .unwrap(),
            "Before  after."
        );
    }

    #[test]
    fn markdown_edits_encode_paths_and_escape_labels() {
        let notes = [
            note("x/source.md", "# Source"),
            note("other/a #b.md", "---\ntitle: 'A [label] *bold*'\n---\n"),
        ];
        let config = ParsingConfig {
            link_format: LinkFormat::Markdown,
            ..Default::default()
        };
        let updated = edit_connection(
            "# Source\n",
            "x/source.md",
            "other/a #b.md",
            &notes,
            &config,
            true,
        )
        .unwrap();
        assert!(updated.contains("[A \\[label\\] \\*bold\\*](../other/a%20%23b.md)"));
    }

    #[test]
    fn creation_ignores_links_sections_inside_code() {
        let notes = [note("a.md", "# A")];
        let content = "```md\n## Links\n[[a]]\n```\n";
        let updated = edit_connection(
            content,
            "source.md",
            "a.md",
            &notes,
            &ParsingConfig::default(),
            true,
        )
        .unwrap();
        assert!(updated.starts_with(content));
        assert_eq!(note("source.md", &updated).links.len(), 1);
    }

    #[test]
    fn atomic_save_checks_staleness_and_preserves_permissions() {
        let notebook = test_support::Notebook::new();
        notebook.write("a.md", "original");
        let original_permissions = std::fs::metadata(notebook.0.join("a.md"))
            .unwrap()
            .permissions();
        save_note(&notebook.0, "a.md", "original", "updated").unwrap();
        assert_eq!(
            std::fs::read_to_string(notebook.0.join("a.md")).unwrap(),
            "updated"
        );
        assert_eq!(
            std::fs::metadata(notebook.0.join("a.md"))
                .unwrap()
                .permissions(),
            original_permissions
        );
        assert!(save_note(&notebook.0, "a.md", "original", "stale overwrite").is_err());
        assert_eq!(
            std::fs::read_to_string(notebook.0.join("a.md")).unwrap(),
            "updated"
        );
        assert_eq!(std::fs::read_dir(&notebook.0).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn save_refuses_readonly_notes_and_symlinks_outside_notebook() {
        let notebook = test_support::Notebook::new();
        let outside = test_support::Notebook::new();
        notebook.write("readonly.md", "read only");
        let path = notebook.0.join("readonly.md");
        let original = std::fs::metadata(&path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        std::fs::set_permissions(&path, readonly).unwrap();
        assert!(save_note(&notebook.0, "readonly.md", "read only", "bad").is_err());
        std::fs::set_permissions(&path, original).unwrap();
        outside.write("other.md", "outside");
        std::os::unix::fs::symlink(outside.0.join("other.md"), notebook.0.join("link.md")).unwrap();
        assert!(save_note(&notebook.0, "link.md", "outside", "bad").is_err());
        assert_eq!(
            std::fs::read_to_string(outside.0.join("other.md")).unwrap(),
            "outside"
        );
    }
}

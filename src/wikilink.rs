//! Content-level wikilink editing shared by hosts. Pure string transforms:
//! hosts own file IO (loading, saving, frontmatter sync).

/// Append `[[target_title]]` under the note's `## Links` section (creating the
/// section if absent). Returns the content unchanged when the link already
/// exists.
pub fn add_wikilink(content: &str, target_title: &str) -> String {
    let link = format!("[[{target_title}]]");
    if content.contains(&link) {
        return content.to_string();
    }

    let mut content = content.to_string();
    if let Some(idx) = content.find("\n## Links\n") {
        content.insert_str(idx + "\n## Links\n".len(), &format!("{link}\n"));
    } else if let Some(idx) = content.find("\n## Links") {
        if idx + "\n## Links".len() == content.len() {
            content.push_str(&format!("\n{link}\n"));
        } else {
            let ensure_newline = if content.ends_with('\n') { "" } else { "\n" };
            content.push_str(&format!("{ensure_newline}\n## Links\n{link}\n"));
        }
    } else {
        let ensure_newline = if content.ends_with('\n') { "" } else { "\n" };
        content.push_str(&format!("{ensure_newline}\n## Links\n{link}\n"));
    }
    content
}

/// Remove every `[[target…]]` link whose target name matches
/// `target_title` (case-insensitive, alias-aware), trimming a trailing empty
/// `## Links` heading. Non-matching links are preserved verbatim.
pub fn remove_wikilink(content: &str, target_title: &str) -> String {
    let pattern = format!("[[{target_title}");
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find(&pattern) {
        let after = &rest[start + 2..];
        if let Some(end) = after.find("]]") {
            let inner = &after[..end];
            let name = match inner.find('|') {
                Some(p) => &inner[..p],
                None => inner,
            }
            .trim();
            if name.eq_ignore_ascii_case(target_title) {
                let mut prefix = &rest[..start];
                if prefix.ends_with(' ') {
                    prefix = prefix.trim_end_matches(' ');
                }
                out.push_str(prefix);

                let rest_after = &after[end + 2..];
                let consume_newline =
                    rest_after.starts_with('\n') || rest_after.starts_with("\r\n");

                if consume_newline && prefix.ends_with('\n') {
                    rest = if let Some(stripped) = rest_after.strip_prefix("\r\n") {
                        stripped
                    } else {
                        &rest_after[1..]
                    };
                } else {
                    rest = rest_after;
                }
                continue;
            }
        }
        out.push_str(&rest[..start + pattern.len()]);
        rest = &rest[start + pattern.len()..];
    }
    out.push_str(rest);

    let trimmed = out.trim_end();
    if trimmed.ends_with("## Links") {
        let new_len = trimmed.len() - "## Links".len();
        let mut new_out = trimmed[..new_len].trim_end().to_string();
        if !new_out.is_empty() {
            new_out.push('\n');
        }
        out = new_out;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_then_remove_roundtrips() {
        let original = "---\ntitle: A\n---\n\nBody text.\n";
        let added = add_wikilink(original, "Some Note");
        assert!(added.contains("\n## Links\n[[Some Note]]\n"));
        // Adding twice does not duplicate.
        let added_twice = add_wikilink(&added, "Some Note");
        assert_eq!(added.matches("[[Some Note]]").count(), 1);
        assert_eq!(added, added_twice);
        // Removal restores the original content (empty Links section trimmed).
        assert_eq!(remove_wikilink(&added, "Some Note"), original);
    }

    #[test]
    fn remove_is_alias_aware_keeps_others() {
        let content = "Intro\n\n## Links\n- [[Target Note|alias]]\n- [[other]]\n- [[target not]]\n";
        let out = remove_wikilink(content, "Target Note");
        assert!(!out.contains("[[Target Note|alias]]"), "alias link removed");
        assert!(out.contains("[[other]]"), "unrelated link kept");
        // ponytail: prefix search is case-sensitive (clin parity); a
        // case-insensitive target is NOT removed.
        assert!(out.contains("[[target not]]"), "case-variant kept");
        assert!(!out.trim_end().ends_with("## Links"), "empty section trimmed");
    }

    #[test]
    fn remove_nonexistent_link_is_noop() {
        let content = "Body\n\n## Links\n- [[keep me]]\n";
        assert_eq!(remove_wikilink(content, "Missing"), content);
    }
}

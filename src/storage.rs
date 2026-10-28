use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

pub fn note_path(base: &Path, relative: &str) -> Result<PathBuf> {
    let base = base
        .canonicalize()
        .context("cannot resolve note directory")?;
    let path = base
        .join(relative)
        .canonicalize()
        .context("cannot resolve note path")?;
    ensure!(
        path.starts_with(&base) && path.is_file(),
        "note is outside the scan directory or not a file"
    );
    Ok(path)
}

pub fn edit_note(base: &Path, relative: &str, edit: impl FnOnce(&str) -> String) -> Result<String> {
    let path = note_path(base, relative)?;
    let original = fs::read_to_string(&path).context("cannot read note")?;
    let permissions = fs::metadata(&path)?.permissions();
    ensure!(!permissions.readonly(), "note is read-only");
    let replacement = edit(&original);
    if replacement == original {
        return Ok(original);
    }

    let staged = path.with_file_name(format!(
        ".{}.graf-save",
        path.file_name().unwrap().to_string_lossy()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // The staging file also excludes another graf writer; never overwrite it.
    let mut file = options
        .open(&staged)
        .context("cannot stage note; another save or a stale .graf-save file exists")?;
    let mut renamed = false;
    let result = (|| {
        file.write_all(replacement.as_bytes())?;
        file.set_permissions(permissions)?;
        file.sync_all()?;
        // shortcut: external editors may race the final check; transactional hosts need shared locking.
        ensure!(
            fs::read_to_string(&path)? == original,
            "note changed during save; retry after refreshing"
        );
        fs::rename(&staged, &path)?;
        renamed = true;
        #[cfg(unix)]
        fs::File::open(path.parent().unwrap())?.sync_all()?;
        Ok(replacement)
    })();
    if !renamed {
        let _ = fs::remove_file(&staged);
    }
    result.context("cannot save note")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    #[test]
    fn saves_atomically_and_rejects_conflicting_writers() {
        let temp = TestDir::new();
        let path = temp.path().join("a.md");
        fs::write(&path, "original").unwrap();
        assert_eq!(
            edit_note(temp.path(), "a.md", |_| "saved".into()).unwrap(),
            "saved"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "saved");
        fs::write(temp.path().join(".a.md.graf-save"), "other writer").unwrap();
        assert!(edit_note(temp.path(), "a.md", |_| "bad".into()).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "saved");
    }

    #[test]
    fn preserves_external_changes_and_cleans_failed_stage() {
        let temp = TestDir::new();
        let path = temp.path().join("a.md");
        fs::write(&path, "original").unwrap();
        assert!(
            edit_note(temp.path(), "a.md", |_| {
                fs::write(&path, "editor change").unwrap();
                "graf change".into()
            })
            .is_err()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "editor change");
        assert!(!temp.path().join(".a.md.graf-save").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_escaping_symlinks_and_preserves_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = TestDir::new();
        let outside = TestDir::new();
        let path = outside.path().join("a.md");
        fs::write(&path, "outside").unwrap();
        symlink(&path, temp.path().join("link.md")).unwrap();
        assert!(edit_note(temp.path(), "link.md", |_| "bad".into()).is_err());
        let path = temp.path().join("a.md");
        fs::write(&path, "original").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        edit_note(temp.path(), "a.md", |_| "saved".into()).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o440)).unwrap();
        assert!(edit_note(temp.path(), "a.md", |_| "bad".into()).is_err());
    }
}

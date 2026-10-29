//! Loader of `content/characters` (003 RF12, RF13).
//!
//! Contract: this crate only does I/O and conversion. Every game rule (ranges, budget,
//! uniqueness and code patterns) lives in the engine (`CharacterSpec::validate`,
//! `is_valid_slug`), so the WASM bindings validate exactly the same. No logging, no async
//! runtime: a caller that needs either wraps [`load_roster`] itself.
//!
//! Layout of the root directory:
//! - regular files (`README.md`) are ignored;
//! - each directory is a character: `<slug>/character.json`, `<slug>/lore.<locale>.md` for
//!   every locale of [`LOCALES`], and anything else is ignored (sprite sets live in
//!   `content/sprites/`, and `just check` flags a `sprites/` folder here, 007 RF-B6);
//! - any other entry, symlinks included, is an error: links are never followed. `character.json`
//!   is checked again on the opened descriptor, so swapping it for a link, a fifo or another file
//!   between the check and the read is also `unexpected_entry`.
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use engine::{CharacterSpec, Ruleset, is_valid_slug};

mod error;
mod file;

pub use error::ContentError;

use file::CharacterFile;

/// Locales that must have a `lore.<locale>.md` (ADR-0007). Keep in sync with `locales` in
/// `apps/web/project.inlang/settings.json`: a new language adds its lore here too.
pub const LOCALES: [&str; 2] = ["en", "es"];

/// Largest `character.json` that is parsed (64 KiB). The read itself is bounded, so a larger
/// file is never loaded whole into memory.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

const CHARACTER_FILE: &str = "character.json";

/// A character of the roster, validated against the ruleset it was loaded with.
///
/// Only [`load_roster`] builds it, so "validated" is an invariant of the type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    spec: CharacterSpec,
    archetype: String,
    rivals: Vec<String>,
    sprites: Option<String>,
}

impl Character {
    /// The engine shape (what `simulate` and `canonical_spec_json` take).
    #[must_use]
    pub const fn spec(&self) -> &CharacterSpec {
        &self.spec
    }

    /// Code of the archetype it evokes, for the lore; the engine ignores it.
    #[must_use]
    pub fn archetype(&self) -> &str {
        &self.archetype
    }

    /// Slugs of its rivals in the same roster, for the narrator; the engine ignores them.
    #[must_use]
    pub fn rivals(&self) -> &[String] {
        &self.rivals
    }

    /// Id of the sprite set in `content/sprites/` (007 RF-B5); the pattern and the existence are
    /// checked by `just check`, not here.
    #[must_use]
    pub fn sprites(&self) -> Option<&str> {
        self.sprites.as_deref()
    }

    #[must_use]
    pub fn into_spec(self) -> CharacterSpec {
        self.spec
    }
}

/// Loads every character under `root`, validated against `ruleset`, sorted by `slug`.
///
/// An empty root gives `Ok(vec![])`: the minimum roster size is the caller's rule.
///
/// # Errors
/// The first problem found, walking the entries in byte order of their names (so the error is
/// the same on every file system). See [`ContentError`].
pub fn load_roster(root: &Path, ruleset: &Ruleset) -> Result<Vec<Character>, ContentError> {
    let mut names = Vec::new();
    // The root itself cannot be named relative to itself, so its I/O errors carry `root`.
    for entry in fs::read_dir(root).map_err(|e| io_error(root.to_path_buf(), &e))? {
        let entry = entry.map_err(|e| io_error(root.to_path_buf(), &e))?;
        names.push(entry.file_name());
    }
    names.sort();

    let mut roster = Vec::new();
    for name in names {
        let rel = PathBuf::from(&name);
        let kind = entry_kind(root, &rel)?;
        match kind {
            EntryKind::File(_) => {}
            EntryKind::Missing | EntryKind::Other => {
                return Err(ContentError::UnexpectedEntry { path: rel });
            }
            EntryKind::Dir => {
                let Some(dir) = name.to_str() else {
                    return Err(ContentError::NameNotUtf8 { path: rel });
                };
                roster.push(load_character(root, dir, ruleset)?);
            }
        }
    }
    roster.sort_by(|x, y| x.spec.slug.cmp(&y.spec.slug));
    check_rivals(&roster)?;
    Ok(roster)
}

enum EntryKind {
    /// With its metadata, to check that the file later opened is the same one.
    File(Metadata),
    Dir,
    Missing,
    /// A symlink, socket, fifo or device.
    Other,
}

/// Kind of `root/rel` without following symlinks.
fn entry_kind(root: &Path, rel: &Path) -> Result<EntryKind, ContentError> {
    match fs::symlink_metadata(root.join(rel)) {
        Ok(meta) if meta.file_type().is_file() => Ok(EntryKind::File(meta)),
        Ok(meta) if meta.file_type().is_dir() => Ok(EntryKind::Dir),
        Ok(_) => Ok(EntryKind::Other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(EntryKind::Missing),
        Err(e) => Err(io_error(rel.to_path_buf(), &e)),
    }
}

fn load_character(root: &Path, dir: &str, ruleset: &Ruleset) -> Result<Character, ContentError> {
    let path = Path::new(dir).join(CHARACTER_FILE);
    let checked = match entry_kind(root, &path)? {
        EntryKind::File(meta) => meta,
        EntryKind::Missing => return Err(ContentError::MissingCharacterFile { path }),
        EntryKind::Dir | EntryKind::Other => return Err(ContentError::UnexpectedEntry { path }),
    };
    let bytes = read_bounded(root, &path, &checked)?;
    let file: CharacterFile = serde_json::from_slice(&bytes).map_err(|e| json_error(&path, &e))?;
    if file.slug != dir {
        return Err(ContentError::SlugMismatch { path });
    }
    let slug = file.slug.clone();
    if !is_valid_slug(&file.archetype) {
        return Err(ContentError::ArchetypeInvalid { path, slug });
    }
    if file.ruleset_version != ruleset.version() {
        return Err(ContentError::RulesetVersionMismatch {
            path,
            slug,
            found: file.ruleset_version,
            expected: ruleset.version(),
        });
    }
    for locale in LOCALES {
        let lore = Path::new(dir).join(format!("lore.{locale}.md"));
        match entry_kind(root, &lore)? {
            EntryKind::File(_) => {}
            EntryKind::Missing => {
                return Err(ContentError::MissingLore {
                    path: lore,
                    slug,
                    locale,
                });
            }
            EntryKind::Dir | EntryKind::Other => {
                return Err(ContentError::UnexpectedEntry { path: lore });
            }
        }
    }
    let file::FileParts {
        spec,
        archetype,
        rivals,
        sprites,
    } = file.into_parts();
    spec.validate(ruleset)
        .map_err(|source| ContentError::Spec {
            path: path.clone(),
            slug: slug.clone(),
            source,
        })?;
    Ok(Character {
        spec,
        archetype,
        rivals,
        sprites,
    })
}

/// Reads at most `MAX_FILE_BYTES + 1` bytes: one more than the cap means the file is too large,
/// whatever its metadata says. `checked` is what `entry_kind` saw; see [`open_checked`].
fn read_bounded(root: &Path, rel: &Path, checked: &Metadata) -> Result<Vec<u8>, ContentError> {
    let file = open_checked(&root.join(rel), checked).map_err(|e| match e {
        OpenError::NotTheCheckedFile => ContentError::UnexpectedEntry {
            path: rel.to_path_buf(),
        },
        OpenError::Io(e) => io_error(rel.to_path_buf(), &e),
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(rel.to_path_buf(), &e))?;
    if u64::try_from(bytes.len()).map_or(true, |len| len > MAX_FILE_BYTES) {
        return Err(ContentError::FileTooLarge {
            path: rel.to_path_buf(),
        });
    }
    Ok(bytes)
}

#[derive(Debug)]
enum OpenError {
    /// A symlink, a fifo, a device, a directory or another file than the one checked.
    NotTheCheckedFile,
    Io(io::Error),
}

/// Opens `path` for reading only if it is still the regular file `checked` describes (closes the
/// window between `symlink_metadata` and `open`). On unix the open does not follow a final
/// symlink (`O_NOFOLLOW`) nor block on a fifo (`O_NONBLOCK`, harmless for a regular file), and
/// the descriptor must have the device and inode of `checked`. Elsewhere only the file type of
/// the descriptor is checked.
fn open_checked(path: &Path, checked: &Metadata) -> Result<File, OpenError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        #[cfg(unix)]
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => {
            return Err(OpenError::NotTheCheckedFile);
        }
        Err(e) => return Err(OpenError::Io(e)),
    };
    let opened = file.metadata().map_err(OpenError::Io)?;
    if opened.file_type().is_file() && same_file(&opened, checked) {
        Ok(file)
    } else {
        Err(OpenError::NotTheCheckedFile)
    }
}

#[cfg(unix)]
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
const fn same_file(_: &Metadata, _: &Metadata) -> bool {
    true
}

/// Rivals must be other characters of the same roster, each listed once.
fn check_rivals(roster: &[Character]) -> Result<(), ContentError> {
    for character in roster {
        let slug = &character.spec.slug;
        let path = || Path::new(slug).join(CHARACTER_FILE);
        for (i, rival) in character.rivals.iter().enumerate() {
            if rival == slug {
                return Err(ContentError::RivalSelf {
                    path: path(),
                    slug: slug.clone(),
                });
            }
            if character.rivals.iter().take(i).any(|other| other == rival) {
                return Err(ContentError::RivalDuplicate {
                    path: path(),
                    slug: slug.clone(),
                    rival: rival.clone(),
                });
            }
            if !roster.iter().any(|other| &other.spec.slug == rival) {
                return Err(ContentError::RivalUnknown {
                    path: path(),
                    slug: slug.clone(),
                    rival: rival.clone(),
                });
            }
        }
    }
    Ok(())
}

fn io_error(path: PathBuf, error: &io::Error) -> ContentError {
    ContentError::Io {
        path,
        kind: error.kind(),
    }
}

/// Position and class of the error, never the message of `serde_json`.
fn json_error(path: &Path, error: &serde_json::Error) -> ContentError {
    let (path, line, column) = (path.to_path_buf(), error.line(), error.column());
    match error.classify() {
        serde_json::error::Category::Data => ContentError::InvalidShape { path, line, column },
        serde_json::error::Category::Io
        | serde_json::error::Category::Syntax
        | serde_json::error::Category::Eof => ContentError::InvalidJson { path, line, column },
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::process::Command;

    use super::*;

    fn ok<T>(result: io::Result<T>, what: &str) -> T {
        result.unwrap_or_else(|e| panic!("{what}: {e}"))
    }

    /// A fresh directory with `checked.json` (the file `entry_kind` saw) and `other.json`.
    fn setup(name: &str) -> (PathBuf, Metadata) {
        let dir = std::env::temp_dir()
            .join(format!("content-open-checked-{}", std::process::id()))
            .join(name);
        if dir.exists() {
            ok(fs::remove_dir_all(&dir), "clean");
        }
        ok(fs::create_dir_all(&dir), "mkdir");
        for name in ["checked.json", "other.json"] {
            ok(fs::write(dir.join(name), "{}"), "write");
        }
        let meta = ok(fs::symlink_metadata(dir.join("checked.json")), "lstat");
        (dir, meta)
    }

    fn not_the_checked_file(result: Result<File, OpenError>) -> bool {
        matches!(result, Err(OpenError::NotTheCheckedFile))
    }

    #[test]
    fn the_unchanged_file_opens() {
        let (dir, meta) = setup("unchanged");
        assert!(open_checked(&dir.join("checked.json"), &meta).is_ok());
    }

    #[test]
    fn a_file_swapped_for_a_symlink_is_rejected() {
        let (dir, meta) = setup("symlink");
        let path = dir.join("checked.json");
        fs::remove_file(&path).expect("remove");
        symlink(dir.join("other.json"), &path).expect("symlink");
        assert!(not_the_checked_file(open_checked(&path, &meta)));
    }

    #[test]
    fn a_file_swapped_for_a_symlink_to_itself_is_rejected() {
        // Same inode behind the link: only O_NOFOLLOW catches it.
        let (dir, meta) = setup("symlink-same");
        let path = dir.join("checked.json");
        fs::rename(&path, dir.join("moved.json")).expect("move");
        symlink(dir.join("moved.json"), &path).expect("symlink");
        assert!(not_the_checked_file(open_checked(&path, &meta)));
    }

    #[test]
    fn a_file_swapped_for_another_file_is_rejected() {
        let (dir, meta) = setup("other-file");
        let path = dir.join("checked.json");
        fs::rename(dir.join("other.json"), &path).expect("rename");
        assert!(not_the_checked_file(open_checked(&path, &meta)));
    }

    #[test]
    fn a_file_swapped_for_a_fifo_is_rejected_without_blocking() {
        let (dir, meta) = setup("fifo");
        let path = dir.join("checked.json");
        fs::remove_file(&path).expect("remove");
        let status = Command::new("mkfifo").arg(&path).status().expect("mkfifo");
        assert!(status.success());
        assert!(not_the_checked_file(open_checked(&path, &meta)));
    }
}

//! Why a roster cannot be loaded (plan-b decision 3).
//!
//! Every variant carries the path relative to the content root (the root itself, as given,
//! when reading the root fails) and, from `archetype_invalid` on, the `slug` as written in
//! `character.json`. It is the directory name (`slug_mismatch` comes first) but may still be
//! malformed: `spec` with `SpecError::SlugInvalid` carries the invalid slug. This is
//! repository content, not user data, and it says which file to fix. No variant carries the text of `serde_json` or of the operating system (ADR-0007,
//! ADR-0012): only codes, positions and `io::ErrorKind`.
use std::io;
use std::path::PathBuf;

use engine::SpecError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContentError {
    /// A read failed. Keeps the kind, never the OS message.
    #[error("io ({kind:?}) at {}", path.display())]
    Io { path: PathBuf, kind: io::ErrorKind },
    /// A directory entry name is not UTF-8.
    #[error("name_not_utf8 at {}", path.display())]
    NameNotUtf8 { path: PathBuf },
    /// A symlink or anything else that is neither a regular file nor a directory where one
    /// was expected. Links are never followed out of `content/`.
    #[error("unexpected_entry at {}", path.display())]
    UnexpectedEntry { path: PathBuf },
    /// A character directory without `character.json`.
    #[error("missing_character_file at {}", path.display())]
    MissingCharacterFile { path: PathBuf },
    /// `character.json` is larger than [`crate::MAX_FILE_BYTES`].
    #[error("file_too_large at {}", path.display())]
    FileTooLarge { path: PathBuf },
    /// Not JSON (syntax error or unexpected end of file).
    #[error("invalid_json at {}:{line}:{column}", path.display())]
    InvalidJson {
        path: PathBuf,
        line: usize,
        column: usize,
    },
    /// JSON with the wrong shape: an unknown or missing field, or a value of the wrong type.
    #[error("invalid_shape at {}:{line}:{column}", path.display())]
    InvalidShape {
        path: PathBuf,
        line: usize,
        column: usize,
    },
    /// `slug` is not the name of the directory.
    #[error("slug_mismatch at {}", path.display())]
    SlugMismatch { path: PathBuf },
    /// `archetype` does not match the slug pattern.
    #[error("archetype_invalid at {} ({slug})", path.display())]
    ArchetypeInvalid { path: PathBuf, slug: String },
    /// `ruleset_version` is not the version of the ruleset it is loaded with.
    #[error("ruleset_version_mismatch at {} ({slug}): {found}, expected {expected}", path.display())]
    RulesetVersionMismatch {
        path: PathBuf,
        slug: String,
        found: u16,
        expected: u16,
    },
    /// `lore.<locale>.md` does not exist.
    #[error("missing_lore at {} ({slug}, {locale})", path.display())]
    MissingLore {
        path: PathBuf,
        slug: String,
        locale: &'static str,
    },
    /// The engine rejects the character (`CharacterSpec::validate`).
    #[error("spec ({source}) at {} ({slug})", path.display())]
    Spec {
        path: PathBuf,
        slug: String,
        source: SpecError,
    },
    /// A rival that is not in the roster.
    #[error("rival_unknown at {} ({slug}): {rival}", path.display())]
    RivalUnknown {
        path: PathBuf,
        slug: String,
        rival: String,
    },
    /// A character that is its own rival.
    #[error("rival_self at {} ({slug})", path.display())]
    RivalSelf { path: PathBuf, slug: String },
    /// A rival listed twice.
    #[error("rival_duplicate at {} ({slug}): {rival}", path.display())]
    RivalDuplicate {
        path: PathBuf,
        slug: String,
        rival: String,
    },
}

impl ContentError {
    /// Stable `snake_case` code (what a log line records, with the path).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::NameNotUtf8 { .. } => "name_not_utf8",
            Self::UnexpectedEntry { .. } => "unexpected_entry",
            Self::MissingCharacterFile { .. } => "missing_character_file",
            Self::FileTooLarge { .. } => "file_too_large",
            Self::InvalidJson { .. } => "invalid_json",
            Self::InvalidShape { .. } => "invalid_shape",
            Self::SlugMismatch { .. } => "slug_mismatch",
            Self::ArchetypeInvalid { .. } => "archetype_invalid",
            Self::RulesetVersionMismatch { .. } => "ruleset_version_mismatch",
            Self::MissingLore { .. } => "missing_lore",
            Self::Spec { .. } => "spec",
            Self::RivalUnknown { .. } => "rival_unknown",
            Self::RivalSelf { .. } => "rival_self",
            Self::RivalDuplicate { .. } => "rival_duplicate",
        }
    }

    /// The path, relative to the content root, of the entry that failed (the root as given
    /// when the root itself cannot be read).
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        match self {
            Self::Io { path, .. }
            | Self::NameNotUtf8 { path }
            | Self::UnexpectedEntry { path }
            | Self::MissingCharacterFile { path }
            | Self::FileTooLarge { path }
            | Self::InvalidJson { path, .. }
            | Self::InvalidShape { path, .. }
            | Self::SlugMismatch { path }
            | Self::ArchetypeInvalid { path, .. }
            | Self::RulesetVersionMismatch { path, .. }
            | Self::MissingLore { path, .. }
            | Self::Spec { path, .. }
            | Self::RivalUnknown { path, .. }
            | Self::RivalSelf { path, .. }
            | Self::RivalDuplicate { path, .. } => path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_snake_case_and_lead_the_display() {
        let p = PathBuf::from("x/character.json");
        let s = || "x".to_owned();
        let all = [
            ContentError::Io {
                path: p.clone(),
                kind: io::ErrorKind::PermissionDenied,
            },
            ContentError::NameNotUtf8 { path: p.clone() },
            ContentError::UnexpectedEntry { path: p.clone() },
            ContentError::MissingCharacterFile { path: p.clone() },
            ContentError::FileTooLarge { path: p.clone() },
            ContentError::InvalidJson {
                path: p.clone(),
                line: 1,
                column: 2,
            },
            ContentError::InvalidShape {
                path: p.clone(),
                line: 1,
                column: 2,
            },
            ContentError::SlugMismatch { path: p.clone() },
            ContentError::ArchetypeInvalid {
                path: p.clone(),
                slug: s(),
            },
            ContentError::RulesetVersionMismatch {
                path: p.clone(),
                slug: s(),
                found: 2,
                expected: 1,
            },
            ContentError::MissingLore {
                path: p.clone(),
                slug: s(),
                locale: "es",
            },
            ContentError::Spec {
                path: p.clone(),
                slug: s(),
                source: SpecError::BudgetExceeded,
            },
            ContentError::RivalUnknown {
                path: p.clone(),
                slug: s(),
                rival: s(),
            },
            ContentError::RivalSelf {
                path: p.clone(),
                slug: s(),
            },
            ContentError::RivalDuplicate {
                path: p.clone(),
                slug: s(),
                rival: s(),
            },
        ];
        // A new variant fails to compile in code() and path(); this count catches a variant
        // that is added there but not here.
        assert_eq!(all.len(), 15);
        let codes: Vec<&str> = all.iter().map(ContentError::code).collect();
        for (i, (error, code)) in all.iter().zip(&codes).enumerate() {
            assert!(
                code.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_'),
                "{code}"
            );
            assert!(!codes.iter().take(i).any(|other| other == code), "{code}");
            let shown = error.to_string();
            let code_shown = if *code == "io" { "io (" } else { code };
            assert!(shown.starts_with(code_shown), "{shown}");
            assert!(shown.contains("x/character.json"), "{shown}");
            assert_eq!(error.path(), p.as_path());
        }
    }
}

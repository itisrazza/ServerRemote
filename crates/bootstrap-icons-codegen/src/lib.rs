//! Shared logic used by both `bootstrap-icons`'s build script and the
//! `bootstrap-icons-macros` proc macros, so the two stay in agreement on how
//! an icon's kebab-case file name maps to its generated Rust identifier.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Converts a kebab-case icon name (e.g. `gear-wide-connected`) into the
/// PascalCase identifier used for its `Icons` enum variant (e.g.
/// `GearWideConnected`).
///
/// Bootstrap Icons has a few names that are entirely numeric segments (e.g.
/// `0-circle-fill`, `123`), which would otherwise produce an identifier
/// starting with a digit; those are prefixed with an underscore.
pub fn to_pascal_ident(kebab: &str) -> String {
    let mut ident: String = kebab
        .split('-')
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect();

    if ident.starts_with(|c: char| c.is_ascii_digit()) {
        ident.insert(0, '_');
    }

    ident
}

/// A single discovered icon: its kebab-case name and the absolute path to
/// its SVG source file.
pub struct IconEntry {
    pub name: String,
    pub path: PathBuf,
}

/// Scans `dir` for `.svg` files and returns their icon entries, sorted by
/// name for deterministic codegen output.
pub fn discover_svgs(dir: &Path) -> io::Result<Vec<IconEntry>> {
    let mut icons: Vec<IconEntry> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?.to_owned();
            (path.extension().and_then(|ext| ext.to_str()) == Some("svg"))
                .then_some(IconEntry { name, path })
        })
        .collect();

    icons.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(icons)
}

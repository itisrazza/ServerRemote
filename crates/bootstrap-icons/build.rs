use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let icons_dir = manifest_dir.join("vendor/bootstrap-icons/icons");
    println!("cargo::rerun-if-changed={}", icons_dir.display());

    let icons = bootstrap_icons_codegen::discover_svgs(&icons_dir).unwrap_or_else(|error| {
        panic!(
            "failed to read Bootstrap Icons from {} (submodule not checked out?): {error}",
            icons_dir.display()
        )
    });
    assert!(
        !icons.is_empty(),
        "found no SVGs under {} - is the bootstrap-icons submodule checked out?",
        icons_dir.display()
    );

    let mut code = String::new();

    code.push_str("/// A single Bootstrap Icon.\n");
    code.push_str("///\n");
    code.push_str("/// Generated at build time from the vendored `twbs/icons` submodule.\n");
    code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    code.push_str("pub enum Icons {\n");
    for icon in &icons {
        let ident = bootstrap_icons_codegen::to_pascal_ident(&icon.name);
        writeln!(code, "    {ident},").unwrap();
    }
    code.push_str("}\n\n");

    code.push_str("impl Icons {\n");
    code.push_str("    /// The icon's raw SVG source.\n");
    code.push_str("    pub fn svg(&self) -> &'static [u8] {\n");
    code.push_str("        match self {\n");
    for icon in &icons {
        let ident = bootstrap_icons_codegen::to_pascal_ident(&icon.name);
        let path = icon.path.to_str().expect("non-UTF-8 icon path");
        writeln!(
            code,
            "            Self::{ident} => include_bytes!({path:?}),"
        )
        .unwrap();
    }
    code.push_str("        }\n");
    code.push_str("    }\n");
    code.push_str("}\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("icons.rs"), code).expect("failed to write generated icons.rs");
}

// packages wesl libraries for processing and lygia
// note that bevy does not (as of yet) understand how to use these packages
// they are, instead, used by us as part of shader pre-processing in the
// processing_render crate
fn main() {
    // the package artifacts are rebuilt when a shader file is added or changed
    println!("cargo:rerun-if-changed=shaders");
    println!("cargo:rerun-if-changed=build.rs");
    wesl::PkgBuilder::new("processing")
        .scan_root("shaders/processing")
        .expect("failed to scan processing WESL files")
        .build_artifact()
        .expect("failed to build processing package artifact");

    // lygia has folders wesl can't name (2mat3, const, .github, tests with
    // dashes); it skips them with a warning each. A mirror without them
    // gives the same package, quietly.
    println!("cargo:rerun-if-changed=../../lygia");
    let mirror = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("lygia");
    let _ = std::fs::remove_dir_all(&mirror);
    mirror_wesl(std::path::Path::new("../../lygia"), &mirror).expect("failed to mirror Lygia");
    wesl::PkgBuilder::new("lygia")
        .scan_root(&mirror)
        .expect("failed to scan Lygia WESL files")
        .build_artifact()
        .expect("failed to build Lygia package artifact");
}

/// Can wesl use this as a module name? (a WGSL identifier, not a keyword)
fn module_name_ok(name: &str) -> bool {
    const KEYWORDS: &[&str] = &["alias", "break", "case", "const", "const_assert", "continue", "continuing", "default",
        "diagnostic", "discard", "else", "enable", "false", "fn", "for", "if", "let", "loop", "override", "requires",
        "return", "struct", "switch", "true", "var", "while"];
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    first_ok && chars.all(|c| c.is_ascii_alphanumeric() || c == '_') && name != "_" && !name.starts_with("__")
        && !KEYWORDS.contains(&name)
}

/// Copies the shader files under `from` to `to` (as links when it can),
/// leaving out what wesl would skip.
fn mirror_wesl(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let path = entry?.path();
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if path.is_dir() {
            if module_name_ok(&stem) { mirror_wesl(&path, &to.join(&stem))?; }
            continue;
        }
        let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        if (ext == "wesl" || ext == "wgsl") && module_name_ok(&stem) {
            let dest = to.join(path.file_name().unwrap());
            let abs = std::fs::canonicalize(&path)?;
            #[cfg(unix)]
            if std::os::unix::fs::symlink(&abs, &dest).is_ok() { continue; }
            std::fs::copy(&abs, &dest)?;
        }
    }
    Ok(())
}

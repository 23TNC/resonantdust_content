//! Auto-discovers card and recipe JSON files at compile time.
//!
//! Recursively walks `cards/data/` and `recipes/data/`, then writes
//! `$OUT_DIR/data_files.rs` containing two `pub const` slices of
//! `(rel_path, contents)` pairs built with `include_str!`. The crate
//! exposes those slices via `crate::embedded_data::*`, so adding,
//! renaming, or nesting a file under either directory needs no
//! source edit — cargo re-runs this script (via the
//! `rerun-if-changed` lines below) and the registries pick the file
//! up on next build.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
  let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
  let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("data_files.rs");

  let mut out = String::new();
  emit_list(&manifest_dir, "cards/data", "CARDS_FILES", &mut out);
  emit_list(&manifest_dir, "recipes/data", "RECIPES_FILES", &mut out);

  fs::write(&out_path, out)
    .unwrap_or_else(|e| panic!("write {}: {}", out_path.display(), e));
}

fn emit_list(manifest_dir: &Path, rel_dir: &str, const_name: &str, out: &mut String) {
  let root = manifest_dir.join(rel_dir);

  let mut files: Vec<(String, PathBuf)> = Vec::new();
  collect_json(&root, &root, &mut files);
  // Sort by path-under-root so the generated slice is deterministic
  // regardless of read_dir's traversal order.
  files.sort_by(|a, b| a.0.cmp(&b.0));

  out.push_str(&format!(
    "pub const {}: &[(&str, &str)] = &[\n",
    const_name
  ));
  for (rel_under_root, abs) in &files {
    // …and re-run when individual file contents change.
    println!("cargo:rerun-if-changed={}", abs.display());
    out.push_str(&format!(
      "    (\"{rel_dir}/{rel}\", include_str!({abs:?})),\n",
      rel_dir = rel_dir,
      rel = rel_under_root,
      abs = abs.display().to_string(),
    ));
  }
  out.push_str("];\n\n");
}

/// Recursively gather every `*.json` file under `dir`, paired with its
/// path relative to `root` (forward-slash separated). Emits a
/// `cargo:rerun-if-changed` for each directory walked so adding /
/// removing files anywhere in the tree triggers a rebuild.
fn collect_json(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
  println!("cargo:rerun-if-changed={}", dir.display());
  let entries = fs::read_dir(dir)
    .unwrap_or_else(|e| panic!("read_dir {}: {}", dir.display(), e));
  for entry in entries.flatten() {
    let Ok(ft) = entry.file_type() else { continue };
    let path = entry.path();
    if ft.is_dir() {
      collect_json(root, &path, out);
      continue;
    }
    if !ft.is_file() {
      continue;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
    if !name.ends_with(".json") {
      continue;
    }
    let rel = path
      .strip_prefix(root)
      .unwrap()
      .to_string_lossy()
      .replace('\\', "/");
    out.push((rel, path));
  }
}

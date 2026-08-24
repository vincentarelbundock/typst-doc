//! The public API of a Typst package, read from its entry point.
//!
//! A file's location says nothing about how it is imported: a package's
//! `lib.typ` re-exports what it chooses, under whatever names it chooses, so
//! `src/component/image.typ` may be reached as `image`, as `layout.image`, or
//! not at all. The manifest names the entry point and the entry point names
//! the API, which makes both facts readable rather than guessable.
//!
//! Two consequences follow, and they are the reason this module exists.
//!
//! - **The public API cannot hold two things with the same name.** Importing
//!   two `image` bindings into one module is legal, but the second shadows the
//!   first, so only one is reachable. Names resolved here are unique by
//!   construction; the path-suffix disambiguation elsewhere is for files that
//!   no entry point speaks for.
//! - **Reachability is what private means.** Not the `_` prefix, which is a
//!   convention, but whether a user of the package can call the thing at all.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use typst_syntax::ast;

/// Where a binding is defined: the file, and the name it is defined under.
pub type Definition = (PathBuf, String);

/// Every documented binding a package exports, mapped to the path a user
/// imports it under.
///
/// `mypkg/src/component/image.typ`'s `image`, re-exported by `lib.typ` as
/// `#import "component/image.typ": image`, maps to `image`; behind
/// `#import "layout/image.typ" as layout` it maps to `layout.image`.
pub type Exports = HashMap<Definition, String>;

/// The entry point of the package rooted at `directory`, if it is one.
///
/// Only `[package] entrypoint` is read. The manifest has more in it, but
/// nothing else here needs any of it, and a hand-read key is one dependency
/// fewer for a file this small and this stable.
pub fn entrypoint(directory: &Path) -> Option<PathBuf> {
    let manifest = std::fs::read_to_string(directory.join("typst.toml")).ok()?;
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line.starts_with("[package]");
            continue;
        }
        let Some(value) = line.strip_prefix("entrypoint") else {
            continue;
        };
        if !in_package {
            continue;
        }
        let value = value.trim_start().strip_prefix('=')?.trim();
        let quoted = value.strip_prefix('"')?;
        let end = quoted.find('"')?;
        return Some(directory.join(&quoted[..end]));
    }
    None
}

/// Walk a package's re-exports, from its entry point outwards.
///
/// Every binding reachable from the entry point is recorded under the name it
/// is reachable by. A binding the walk never reaches is not part of the API,
/// and its absence from the result is what says so.
pub fn exports(entrypoint: &Path) -> Exports {
    let mut walker = Walker {
        exports: Exports::new(),
        visiting: HashSet::new(),
    };
    walker.walk(entrypoint, "");
    walker.exports
}

struct Walker {
    exports: Exports,
    /// Files on the current path, so a cycle of imports terminates.
    visiting: HashSet<PathBuf>,
}

impl Walker {
    /// Record everything `file` re-exports, prefixed by `prefix`.
    fn walk(&mut self, file: &Path, prefix: &str) {
        let here = canonical(file);
        if !self.visiting.insert(here.clone()) {
            return;
        }

        for import in imports(file) {
            let Some(target) = import.target else {
                continue;
            };
            match import.items {
                // `#import "m.typ" as m` and `#import "m.typ"` bind the module
                // itself, so everything in it is reachable behind its name —
                // including what it imported in turn.
                Items::Module(binding) => {
                    let prefix = format!("{prefix}{binding}.");
                    for name in bindings(&target) {
                        self.exports.insert(
                            (canonical(&target), name.clone()),
                            format!("{prefix}{name}"),
                        );
                    }
                    self.walk(&target, &prefix);
                }
                // `#import "m.typ": *` re-exports every name unchanged.
                Items::Wildcard => {
                    for name in bindings(&target) {
                        self.exports.insert(
                            (canonical(&target), name.clone()),
                            format!("{prefix}{name}"),
                        );
                    }
                    self.walk(&target, prefix);
                }
                // `#import "m.typ": a, b as c` re-exports named bindings, which
                // `m.typ` may itself have imported from somewhere deeper.
                Items::Named(named) => {
                    for (original, bound) in named {
                        if let Some(definition) = self.define(&target, &original) {
                            self.exports.insert(definition, format!("{prefix}{bound}"));
                        }
                    }
                }
            }
        }

        self.visiting.remove(&here);
    }

    /// Where `name` is defined, following `file`'s own imports if it is not
    /// defined there.
    fn define(&self, file: &Path, name: &str) -> Option<Definition> {
        if bindings(file).iter().any(|binding| binding == name) {
            return Some((canonical(file), name.to_owned()));
        }

        let mut seen = HashSet::new();
        self.define_deep(file, name, &mut seen)
    }

    fn define_deep(
        &self,
        file: &Path,
        name: &str,
        seen: &mut HashSet<PathBuf>,
    ) -> Option<Definition> {
        if !seen.insert(file.to_path_buf()) {
            return None;
        }
        if bindings(file).iter().any(|binding| binding == name) {
            return Some((canonical(file), name.to_owned()));
        }

        for import in imports(file) {
            let Some(target) = import.target else {
                continue;
            };
            let next = match &import.items {
                Items::Wildcard => Some(name.to_owned()),
                Items::Named(named) => named
                    .iter()
                    .find(|(_, bound)| bound == name)
                    .map(|(original, _)| original.clone()),
                // A module binding is reached by `module.name`, not by `name`.
                Items::Module(_) => None,
            };
            if let Some(next) = next
                && let Some(definition) = self.define_deep(&target, &next, seen)
            {
                return Some(definition);
            }
        }
        None
    }
}

/// One `#import` in a file.
struct Import {
    /// The file imported, or `None` for a package or unreadable path: an
    /// `@preview` import is somebody else's API, not this package's.
    target: Option<PathBuf>,
    items: Items,
}

enum Items {
    /// `#import "m.typ" as m`, or the bare form that binds the file stem.
    Module(String),
    /// `#import "m.typ": *`
    Wildcard,
    /// `#import "m.typ": a, b as c`, as (original, bound) pairs.
    Named(Vec<(String, String)>),
}

/// The imports written at the top level of a file.
fn imports(file: &Path) -> Vec<Import> {
    let Ok(source) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let root = typst_syntax::parse(&source);
    let directory = file.parent().unwrap_or(Path::new("."));

    let mut out = Vec::new();
    collect(&root, &mut |node| {
        let Some(import) = node.cast::<ast::ModuleImport>() else {
            return;
        };
        let ast::Expr::Str(path) = import.source() else {
            // A module bound to an expression is not a path this can follow.
            return;
        };
        let path = path.get().to_string();
        let target = (!path.starts_with('@')).then(|| directory.join(&path));

        let items = match import.imports() {
            None => {
                let binding = import
                    .new_name()
                    .map(|name| name.get().to_string())
                    .or_else(|| {
                        Path::new(&path)
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().into_owned())
                    })
                    .unwrap_or_default();
                Items::Module(binding)
            }
            Some(ast::Imports::Wildcard) => Items::Wildcard,
            Some(ast::Imports::Items(list)) => Items::Named(
                list.iter()
                    .map(|item| {
                        (
                            item.original_name().get().to_string(),
                            item.bound_name().get().to_string(),
                        )
                    })
                    .collect(),
            ),
        };
        out.push(Import { target, items });
    });
    out
}

/// The names a file binds at its top level.
///
/// Undocumented bindings are included: one may be the module that carries a
/// documented one, and a re-export chain has to be followed through it.
fn bindings(file: &Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let root = typst_syntax::parse(&source);

    let mut out = Vec::new();
    collect(&root, &mut |node| {
        if let Some(binding) = node.cast::<ast::LetBinding>() {
            out.extend(
                binding
                    .kind()
                    .bindings()
                    .into_iter()
                    .map(|ident| ident.get().to_string()),
            );
        }
    });
    out
}

/// Visit every node of a parsed file.
fn collect(node: &typst_syntax::SyntaxNode, visit: &mut impl FnMut(&typst_syntax::SyntaxNode)) {
    visit(node);
    for child in node.children() {
        collect(child, visit);
    }
}

/// A path in the one form two readings of it can be compared by.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The packages the inputs belong to, and what they export.
///
/// A run may name a package directory, a file inside one, or neither; the
/// answer to "what is this called, and is it public?" is only available for
/// files an entry point speaks for.
#[derive(Debug, Default)]
pub struct Api {
    exports: Exports,
    /// The entry points read, named in the report about what they leave out.
    pub entrypoints: Vec<PathBuf>,
    /// The package roots found, so a file can be tested for belonging to one.
    roots: Vec<PathBuf>,
}

impl Api {
    /// Find the package each input belongs to and read its exports.
    ///
    /// A package is found by walking up from the input until a `typst.toml`
    /// turns up, so naming `src/lib.typ` or `src/` works as well as naming the
    /// package directory itself.
    pub fn discover(inputs: &[PathBuf]) -> Self {
        let mut api = Self::default();
        for input in inputs {
            let Some(root) = package_root(input) else {
                continue;
            };
            if api.roots.contains(&root) {
                continue;
            }
            let Some(entrypoint) = entrypoint(&root) else {
                continue;
            };
            api.exports.extend(exports(&entrypoint));
            api.entrypoints.push(entrypoint);
            api.roots.push(root);
        }
        api
    }

    /// Whether an entry point speaks for this file, and so whether its absence
    /// from the exports means anything.
    pub fn covers(&self, file: &Path) -> bool {
        let file = canonical(file);
        self.roots
            .iter()
            .any(|root| file.starts_with(canonical(root)))
    }

    /// The path a user imports this binding under.
    pub fn public_name(&self, file: &Path, name: &str) -> Option<&String> {
        self.exports.get(&(canonical(file), name.to_owned()))
    }
}

/// The package directory an input belongs to: the nearest ancestor, itself
/// included, holding a `typst.toml`.
fn package_root(input: &Path) -> Option<PathBuf> {
    let start = canonical(input);
    let mut directory = if start.is_dir() {
        Some(start.as_path())
    } else {
        start.parent()
    };
    while let Some(current) = directory {
        if current.join("typst.toml").is_file() {
            return Some(current.to_path_buf());
        }
        directory = current.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A package on disk, since reading one is the whole point.
    fn package(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("typst-doc-package-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        for (path, contents) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("a file has a parent"))
                .expect("creating the package");
            std::fs::write(&file, contents).expect("writing a file");
        }
        root
    }

    const MANIFEST: &str =
        "[package]\nname = \"mosaic\"\nversion = \"0.1.0\"\nentrypoint = \"src/lib.typ\"\n";

    #[test]
    fn the_manifest_names_the_entry_point() {
        let root = package("manifest", &[("typst.toml", MANIFEST)]);
        assert_eq!(entrypoint(&root), Some(root.join("src/lib.typ")));

        // A key outside [package] is somebody else's.
        let other = package(
            "manifest-other",
            &[("typst.toml", "[tool.x]\nentrypoint = \"nope.typ\"\n")],
        );
        assert_eq!(entrypoint(&other), None);
    }

    #[test]
    fn the_entry_point_names_the_api() {
        let root = package(
            "api",
            &[
                ("typst.toml", MANIFEST),
                (
                    "src/lib.typ",
                    "#import \"component/image.typ\": image\n\
                     #import \"layout/image.typ\" as layout\n\
                     #import \"component/caption.typ\": caption as label-caption\n",
                ),
                ("src/component/image.typ", "#let image(src) = { }\n"),
                ("src/component/caption.typ", "#let caption(body) = { }\n"),
                (
                    "src/layout/image.typ",
                    "#let image(area) = { }\n#let grid(cols) = { }\n",
                ),
                ("src/internal/util.typ", "#let helper() = { }\n"),
            ],
        );
        let exports = exports(&entrypoint(&root).expect("a manifest"));
        let public = |file: &str, name: &str| {
            exports
                .get(&(canonical(&root.join(file)), name.to_owned()))
                .cloned()
        };

        // Two `image` definitions, one API: the module alias separates them,
        // which is the only way Typst can expose both.
        assert_eq!(
            public("src/component/image.typ", "image"),
            Some("image".into())
        );
        assert_eq!(
            public("src/layout/image.typ", "image"),
            Some("layout.image".into())
        );
        // Everything behind a module alias comes with it.
        assert_eq!(
            public("src/layout/image.typ", "grid"),
            Some("layout.grid".into())
        );
        // `as` renames the export, not the definition.
        assert_eq!(
            public("src/component/caption.typ", "caption"),
            Some("label-caption".into())
        );
        // What the entry point never mentions is not public.
        assert_eq!(public("src/internal/util.typ", "helper"), None);
    }

    #[test]
    fn a_re_export_is_followed_to_the_definition() {
        let root = package(
            "chain",
            &[
                ("typst.toml", MANIFEST),
                ("src/lib.typ", "#import \"middle.typ\": deep\n"),
                ("src/middle.typ", "#import \"leaf.typ\": deep\n"),
                ("src/leaf.typ", "#let deep() = { }\n"),
            ],
        );
        let exports = exports(&entrypoint(&root).expect("a manifest"));

        // The name is public, and it belongs to the file that defines it.
        assert_eq!(
            exports.get(&(canonical(&root.join("src/leaf.typ")), "deep".to_owned())),
            Some(&"deep".to_owned())
        );
    }

    #[test]
    fn a_package_import_and_an_import_cycle_both_terminate() {
        let root = package(
            "cycle",
            &[
                ("typst.toml", MANIFEST),
                (
                    "src/lib.typ",
                    "#import \"@preview/other:1.0.0\": thing\n#import \"a.typ\": one\n",
                ),
                ("src/a.typ", "#import \"b.typ\": one\n#let two() = { }\n"),
                ("src/b.typ", "#import \"a.typ\": two\n#let one() = { }\n"),
            ],
        );
        let exports = exports(&entrypoint(&root).expect("a manifest"));

        // `@preview` is somebody else's API and is not followed.
        assert!(!exports.values().any(|name| name == "thing"));
        assert_eq!(
            exports.get(&(canonical(&root.join("src/b.typ")), "one".to_owned())),
            Some(&"one".to_owned())
        );
    }
}

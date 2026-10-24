//! Source-only module inventory for the fixture-surface instrument. This is a
//! lexical walker of declared modules, not macro expansion or cfg evaluation.
//! It visits disabled modules too: fixtures must remain visible on host lanes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct Inventory {
    pub(super) fixtures: BTreeSet<String>,
    pub(super) target_gate: BTreeSet<String>,
    // Each source retains its own file-level gate; children never gate a target.
    source_gates: BTreeMap<PathBuf, BTreeSet<String>>,
}

#[derive(Debug, PartialEq)]
enum Token {
    Word(String),
    Quoted(String),
    Punct(u8),
}

/// Skip comments and string/character bodies before looking for declarations.
/// In particular, diagnostic prose containing `mod missing;` is not a module.
fn tokens(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
        } else if bytes[at..].starts_with(b"//") {
            at += bytes[at..]
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(bytes.len() - at);
        } else if bytes[at..].starts_with(b"/*") {
            at += 2;
            let mut depth = 1;
            while depth != 0 {
                assert!(at < bytes.len(), "unterminated source comment");
                if bytes[at..].starts_with(b"/*") {
                    depth += 1;
                    at += 2;
                } else if bytes[at..].starts_with(b"*/") {
                    depth -= 1;
                    at += 2;
                } else {
                    at += 1;
                }
            }
        } else if let Some((text, end)) = raw_string(source, at) {
            out.push(Token::Quoted(text));
            at = end;
        } else if bytes[at] == b'"' {
            let start = at + 1;
            at = start;
            while at < bytes.len() && bytes[at] != b'"' {
                at += if bytes[at] == b'\\' { 2 } else { 1 };
            }
            assert!(at < bytes.len(), "unterminated source string");
            out.push(Token::Quoted(source[start..at].to_owned()));
            at += 1;
        } else if bytes[at] == b'\''
            && let Some(end) = char_end(source, at)
        {
            at = end;
        } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let start = at;
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            out.push(Token::Word(source[start..at].to_owned()));
        } else {
            out.push(Token::Punct(bytes[at]));
            at += 1;
        }
    }
    out
}

fn raw_string(source: &str, at: usize) -> Option<(String, usize)> {
    let bytes = source.as_bytes();
    let mut quote = at;
    if matches!(bytes.get(quote), Some(b'b' | b'c')) {
        quote += 1;
    }
    if bytes.get(quote) != Some(&b'r') {
        return None;
    }
    quote += 1;
    let hashes = quote;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }
    let closing = format!("\"{}", "#".repeat(quote - hashes));
    let start = quote + 1;
    let end = start + source[start..].find(&closing).expect("raw string closes");
    Some((source[start..end].to_owned(), end + closing.len()))
}

fn char_end(source: &str, at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut end = at + 1;
    if bytes.get(end) == Some(&b'\\') {
        end += 2;
        if bytes.get(end - 1) == Some(&b'u') && bytes.get(end) == Some(&b'{') {
            end += source[end..].find('}')? + 1;
        } else if bytes.get(end - 1) == Some(&b'x') {
            end += 2;
        }
    } else {
        end += source.get(end..)?.chars().next()?.len_utf8();
    }
    (bytes.get(end) == Some(&b'\'')).then_some(end + 1)
}

fn word(token: Option<&Token>, expected: &str) -> bool {
    matches!(token, Some(Token::Word(value)) if value == expected)
}

fn punct(token: Option<&Token>, expected: u8) -> bool {
    token == Some(&Token::Punct(expected))
}

fn group_end(tokens: &[Token], start: usize) -> usize {
    let close = match tokens[start] {
        Token::Punct(b'(') => b')',
        Token::Punct(b'[') => b']',
        Token::Punct(b'{') => b'}',
        _ => panic!("expected a source group"),
    };
    let mut at = start + 1;
    while at < tokens.len() {
        match tokens[at] {
            Token::Punct(b'(' | b'[' | b'{') => at = group_end(tokens, at) + 1,
            Token::Punct(byte) if byte == close => return at,
            _ => at += 1,
        }
    }
    panic!("unterminated source group")
}

fn cfg_features(attribute: &[Token]) -> BTreeSet<String> {
    if !word(attribute.first(), "cfg") {
        return BTreeSet::new();
    }
    attribute
        .windows(3)
        .filter_map(|parts| match parts {
            [Token::Word(name), Token::Punct(b'='), Token::Quoted(value)] if name == "feature" => {
                Some(value.clone())
            }
            _ => None,
        })
        .collect()
}

pub(super) fn read(path: &Path) -> Inventory {
    let mut inventory = Inventory::default();
    inventory.visit(path, true, &mut BTreeSet::new());
    inventory
}

impl Inventory {
    fn visit(&mut self, path: &Path, root: bool, active: &mut BTreeSet<PathBuf>) {
        let path = path
            .canonicalize()
            .unwrap_or_else(|error| panic!("source {}: {error}", path.display()));
        assert!(
            !active.contains(&path),
            "recursive module source {}",
            path.display()
        );
        if self.source_gates.contains_key(&path) {
            return;
        }
        active.insert(path.clone());
        let tokens = tokens(&fs::read_to_string(&path).expect("module source reads"));
        for token in &tokens {
            if let Token::Quoted(value) = token
                && value.starts_with("WEAVER_")
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
            {
                self.fixtures.insert(value.clone());
            }
        }
        let parent = path.parent().unwrap();
        let module_dir = if root || path.file_name().is_some_and(|name| name == "mod.rs") {
            parent.to_owned()
        } else {
            parent.join(path.file_stem().unwrap())
        };
        let gate = self.modules(&tokens, &module_dir, parent, active);
        if root {
            self.target_gate = gate.clone();
        }
        self.source_gates.insert(path.clone(), gate);
        active.remove(&path);
    }

    /// `module_dir` resolves default children; `path_dir` resolves an explicit
    /// path. In a non-mod.rs file these differ until an inline module is entered.
    fn modules(
        &mut self,
        tokens: &[Token],
        module_dir: &Path,
        path_dir: &Path,
        active: &mut BTreeSet<PathBuf>,
    ) -> BTreeSet<String> {
        let mut gate = BTreeSet::new();
        let mut explicit_path = None;
        let mut at = 0;
        while at < tokens.len() {
            if punct(tokens.get(at), b'#') {
                at += 1;
                let inner = punct(tokens.get(at), b'!');
                at += usize::from(inner);
                assert!(punct(tokens.get(at), b'['), "attribute starts a group");
                let end = group_end(tokens, at);
                let attribute = &tokens[at + 1..end];
                if inner {
                    gate.extend(cfg_features(attribute));
                } else if word(attribute.first(), "path") {
                    let [_, Token::Punct(b'='), Token::Quoted(value)] = attribute else {
                        panic!("literal module path required")
                    };
                    assert!(
                        !value.contains('\\'),
                        "escaped module path needs explicit scanner support"
                    );
                    assert!(
                        explicit_path.replace(value.clone()).is_none(),
                        "duplicate module path"
                    );
                } else if word(attribute.first(), "cfg_attr")
                    && attribute.iter().any(|token| word(Some(token), "path"))
                {
                    panic!("conditional module paths need explicit scanner support");
                }
                at = end + 1;
            } else if word(tokens.get(at), "pub") {
                at += 1;
                if punct(tokens.get(at), b'(') {
                    at = group_end(tokens, at) + 1;
                }
            } else if word(tokens.get(at), "mod") {
                let Some(Token::Word(name)) = tokens.get(at + 1) else {
                    panic!("module name required")
                };
                at += 2;
                if punct(tokens.get(at), b';') {
                    let child = if let Some(path) = explicit_path.take() {
                        path_dir.join(path)
                    } else {
                        let file = module_dir.join(format!("{name}.rs"));
                        let directory = module_dir.join(name).join("mod.rs");
                        assert_ne!(
                            file.is_file(),
                            directory.is_file(),
                            "module {name} must resolve exactly once under {}",
                            module_dir.display()
                        );
                        if file.is_file() { file } else { directory }
                    };
                    self.visit(&child, false, active);
                    at += 1;
                } else {
                    assert!(punct(tokens.get(at), b'{'), "module body required");
                    let end = group_end(tokens, at);
                    let directory = explicit_path
                        .take()
                        .map_or_else(|| module_dir.join(name), |path| path_dir.join(path));
                    // This returned gate belongs to the inline module only.
                    self.modules(&tokens[at + 1..end], &directory, &directory, active);
                    at = end + 1;
                }
            } else {
                explicit_path = None;
                at = if matches!(tokens[at], Token::Punct(b'(' | b'[' | b'{')) {
                    group_end(tokens, at) + 1
                } else {
                    at + 1
                };
            }
        }
        gate
    }
}

struct Sources(PathBuf);
impl Sources {
    fn new(files: &[(&str, &str)]) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "weaver-spu-manifest-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let source = Self(root);
        for (name, contents) in files {
            let path = source.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        source
    }

    fn read(&self, name: &str) -> Inventory {
        read(&self.0.join(name))
    }
}
impl Drop for Sources {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn manifest_scan_follows_default_path_and_recursive_children_for_each_target() {
    let files = Sources::new(&[
        (
            "entry.rs",
            "mod common; #[path = \"entry/ste100.rs\"] mod ste100;",
        ),
        ("loaded.rs", "#![cfg(feature = \"gguf\")] mod common;"),
        ("seam.rs", "mod common;"),
        (
            "common/mod.rs",
            "mod deeper; const FILE: &str = \"WEAVER_COMMON\";",
        ),
        (
            "common/deeper.rs",
            "const FILE: &str = \"WEAVER_RECURSIVE\";",
        ),
        (
            "entry/ste100.rs",
            "#[path = \"../common/fixture.rs\"] mod fixture; const FILE: &str = \"WEAVER_ENTRY\";",
        ),
        (
            "common/fixture.rs",
            "const FILE: &str = \"WEAVER_FIXTURE\";",
        ),
        // An unreferenced .rs file contributes nothing to any target.
        ("unused.rs", "const FILE: &str = \"WEAVER_UNUSED\";"),
    ]);
    for target in ["entry.rs", "loaded.rs", "seam.rs"] {
        let scan = files.read(target);
        let mut expected = BTreeSet::from(["WEAVER_COMMON".into(), "WEAVER_RECURSIVE".into()]);
        if target == "entry.rs" {
            expected.extend(["WEAVER_ENTRY".into(), "WEAVER_FIXTURE".into()]);
        }
        assert_eq!(scan.fixtures, expected, "{target}");
        assert_eq!(
            scan.source_gates.len(),
            if target == "entry.rs" { 5 } else { 3 }
        );
    }
}

#[test]
fn manifest_scan_child_and_inline_gates_do_not_become_target_gates() {
    let files = Sources::new(&[
        (
            "entry.rs",
            "#![cfg(\n feature = \"root\"\n)]\nmod child; mod inline { #![cfg(feature = \"inline\")] const FILE: &str = \"WEAVER_INLINE\"; }",
        ),
        (
            "child.rs",
            "#![cfg(feature = \"cuda\")] mod grandchild; const FILE: &str = \"WEAVER_CHILD\";",
        ),
        (
            "child/grandchild.rs",
            "#![cfg(feature = \"gguf\")] const FILE: &str = \"WEAVER_GRANDCHILD\";",
        ),
        ("ungated.rs", "mod child;"),
    ]);
    let scan = files.read("entry.rs");
    assert_eq!(scan.target_gate, BTreeSet::from(["root".into()]));
    assert_eq!(
        scan.source_gates[&files.0.join("child.rs").canonicalize().unwrap()],
        BTreeSet::from(["cuda".into()])
    );
    assert_eq!(
        scan.source_gates[&files.0.join("child/grandchild.rs").canonicalize().unwrap()],
        BTreeSet::from(["gguf".into()])
    );
    assert_eq!(
        scan.fixtures,
        BTreeSet::from([
            "WEAVER_INLINE".into(),
            "WEAVER_CHILD".into(),
            "WEAVER_GRANDCHILD".into()
        ])
    );
    assert!(files.read("ungated.rs").target_gate.is_empty());
}

#[test]
fn manifest_scan_resolves_non_mod_file_and_inline_path_directories() {
    let files = Sources::new(&[
        (
            "target.rs",
            "pub(crate) mod outer; #[path = \"redirect\"] mod inline { #[path = \"chosen.rs\"] mod chosen; }",
        ),
        (
            "outer.rs",
            "mod inner { mod leaf; #[path = \"chosen.rs\"] mod chosen; } #[path = r#\"shared.rs\"#] pub mod shared;",
        ),
        ("outer/inner/leaf.rs", "const A: &str = \"WEAVER_DEFAULT\";"),
        (
            "outer/inner/chosen.rs",
            "const A: &str = \"WEAVER_INLINE_PATH\";",
        ),
        ("shared.rs", "const A: &str = \"WEAVER_FILE_PATH\";"),
        ("redirect/chosen.rs", "const A: &str = \"WEAVER_REDIRECT\";"),
    ]);
    assert_eq!(
        files.read("target.rs").fixtures,
        BTreeSet::from([
            "WEAVER_DEFAULT".into(),
            "WEAVER_INLINE_PATH".into(),
            "WEAVER_FILE_PATH".into(),
            "WEAVER_REDIRECT".into(),
        ])
    );
}

#[test]
fn manifest_scan_ignores_comments_literals_and_unreferenced_paths() {
    let files = Sources::new(&[
        (
            "target.rs",
            r####"
// mod missing; "WEAVER_COMMENT"
/* outer /* nested */ mod missing; "WEAVER_BLOCK_COMMENT" */
const PROSE: &str = "mod missing;";
const RAW: &str = r###"#[path = "missing.rs"] mod missing;"###;
const QUOTE: char = '\'';
const BRACE: char = '{';
const REAL: &str = "WEAVER_ROOT";
mod real;
fn helper<'a>(value: &'a str) -> &'a str { value }
"####,
        ),
        ("real.rs", "const FILE: &str = r#\"WEAVER_REAL\"#;"),
    ]);
    let scan = files.read("target.rs");
    assert_eq!(
        scan.fixtures,
        BTreeSet::from(["WEAVER_ROOT".into(), "WEAVER_REAL".into()])
    );
    assert_eq!(scan.source_gates.len(), 2);
}

#[test]
fn manifest_scan_shared_source_is_visited_once_per_target() {
    let files = Sources::new(&[
        (
            "target.rs",
            "#[path = \"shared.rs\"] mod one; #[path = \"./shared.rs\"] mod two;",
        ),
        (
            "shared.rs",
            "mod child; const FILE: &str = \"WEAVER_SHARED\";",
        ),
        (
            "shared/child.rs",
            "const FILE: &str = \"WEAVER_SHARED_CHILD\";",
        ),
    ]);
    let scan = files.read("target.rs");
    assert_eq!(scan.source_gates.len(), 3);
    assert_eq!(
        scan.fixtures,
        BTreeSet::from(["WEAVER_SHARED".into(), "WEAVER_SHARED_CHILD".into()])
    );
}

#[test]
#[should_panic(expected = "must resolve exactly once")]
fn manifest_scan_missing_module_does_not_silently_disappear() {
    Sources::new(&[("target.rs", "mod missing;")]).read("target.rs");
}

#[test]
#[should_panic(expected = "must resolve exactly once")]
fn manifest_scan_ambiguous_default_module_does_not_pick_one() {
    Sources::new(&[
        ("target.rs", "mod both;"),
        ("both.rs", ""),
        ("both/mod.rs", ""),
    ])
    .read("target.rs");
}

#[test]
#[should_panic(expected = "recursive module source")]
fn manifest_scan_recursive_path_fails_without_infinite_walk() {
    Sources::new(&[("target.rs", "#[path = \"target.rs\"] mod again;")]).read("target.rs");
}

#[test]
fn manifest_scan_actual_common_and_entry_helpers_are_in_their_consuming_targets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for target in ["entry.rs", "loaded.rs", "seam.rs"] {
        let scan = read(&root.join(target));
        assert!(
            scan.source_gates
                .contains_key(&root.join("common/mod.rs").canonicalize().unwrap()),
            "{target}"
        );
        if target == "entry.rs" {
            for child in ["entry/ste100.rs", "common/ste100_fixture.rs"] {
                assert!(
                    scan.source_gates
                        .contains_key(&root.join(child).canonicalize().unwrap()),
                    "{child}"
                );
            }
        }
    }
}

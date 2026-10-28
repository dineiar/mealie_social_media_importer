use std::fs;
use std::path::{Path, PathBuf};

const SHELLS: [&str; 6] = ["sh", "bash", "zsh", "dash", "/bin/sh", "/bin/bash"];

const LIFTING_LINTS: [&str; 4] = [
    "clippy::all",
    "clippy::style",
    "clippy::disallowed_methods",
    "warnings",
];

fn without_string_literals(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        match (in_string, c) {
            (true, '\\') => {
                chars.next();
            }
            (true, '"') => in_string = false,
            (true, _) => {}
            (false, '\'') if chars.peek() == Some(&'"') => {
                chars.next();
                chars.next();
            }
            (false, '"') => in_string = true,
            (false, _) => out.push(c),
        }
    }
    out
}

fn lint_paths(text: &str) -> Vec<String> {
    let squeezed: String = without_string_literals(text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    squeezed
        .split("#[")
        .skip(1)
        .chain(squeezed.split("#![").skip(1))
        .map(|rest| rest.split(']').next().unwrap_or(rest))
        .filter(|attribute| {
            ["allow(", "expect(", "warn("]
                .iter()
                .any(|key| attribute.contains(key))
        })
        .flat_map(|attribute| {
            attribute
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
                .filter(|path| !path.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn lifting_lints(text: &str) -> Vec<String> {
    lint_paths(text)
        .into_iter()
        .filter(|path| LIFTING_LINTS.contains(&path.as_str()))
        .collect()
}

fn sources(dir: &Path) -> Vec<(PathBuf, String)> {
    fs::read_dir(dir)
        .unwrap()
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path)
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = fs::read_to_string(&path).unwrap();
                vec![(path, text)]
            } else {
                Vec::new()
            }
        })
        .collect()
}

fn all_sources() -> Vec<(PathBuf, String)> {
    let found = sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    assert!(found.len() > 3, "found no sources to check");
    found
}

#[test]
fn only_process_rs_lifts_the_ban_on_spawning() {
    let lifting: Vec<_> = all_sources()
        .into_iter()
        .filter(|(_, text)| text.contains("disallowed_methods"))
        .map(|(path, _)| path)
        .collect();

    let [path] = lifting.as_slice() else {
        panic!("expected only src/process.rs to lift the ban, found {lifting:?}");
    };
    assert!(path.ends_with("process.rs"), "{path:?} lifts the ban");
}

#[test]
fn no_source_names_a_shell() {
    for (path, text) in all_sources() {
        for shell in SHELLS {
            assert!(
                !text.contains(&format!("\"{shell}\"")),
                "{path:?} names the shell {shell}"
            );
            assert!(
                !text.contains(&format!("\"{shell} -c")),
                "{path:?} runs {shell} -c"
            );
        }
    }
}

#[test]
fn no_source_but_process_rs_allows_a_lint_group_that_includes_the_ban() {
    for (path, text) in all_sources() {
        if path.ends_with("process.rs") {
            continue;
        }
        let lifting = lifting_lints(&text);
        assert!(
            lifting.is_empty(),
            "{path:?} allows {lifting:?}, which lifts the ban on spawning"
        );
    }
}

#[test]
fn the_scan_catches_every_way_to_allow_a_group_that_includes_the_ban() {
    let violations = [
        "#[allow(clippy::all)]",
        "#![allow(clippy::style)]",
        "#[expect(clippy::disallowed_methods)]",
        "#[warn(warnings)]",
        "#[allow(dead_code, clippy::all)]",
        "#[cfg_attr(test, allow(clippy::style))]",
        "#[allow(\n    clippy::all,\n    reason = \"x\"\n)]",
        "#[allow(clippy :: disallowed_methods)]",
    ];
    for source in violations {
        assert!(!lifting_lints(source).is_empty(), "missed {source}");
    }
}

#[test]
fn the_scan_accepts_attributes_that_only_mention_a_lint_name_in_passing() {
    let innocent = [
        "#[allow(dead_code, reason = \"silences warnings until the downloader uses it\")]",
        "#[expect(clippy::allow_attributes, reason = \"clippy::all stays on\")]",
        "#[allow(clippy::too_many_arguments)]",
        "#[allow(dead_code, reason = \"a ] in the reason, and clippy::style\")]",
    ];
    for source in innocent {
        assert!(lifting_lints(source).is_empty(), "flagged {source}");
    }
}

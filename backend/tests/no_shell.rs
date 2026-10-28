use std::fs;
use std::path::{Path, PathBuf};

const SHELLS: [&str; 6] = ["sh", "bash", "zsh", "dash", "/bin/sh", "/bin/bash"];

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

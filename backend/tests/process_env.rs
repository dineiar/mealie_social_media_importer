use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use std::time::Duration;

use mealie_importer::process::run;

#[tokio::test]
async fn the_child_gets_only_the_variables_it_needs() {
    // SAFETY: this file holds one test, so no other thread reads or writes the environment.
    unsafe {
        env::set_var("TRANSCRIPTION_API_KEY", "planted-key");
        env::set_var("MEALIE_URL", "http://planted.invalid");
        env::set_var("TZ", "Europe/Oslo");
        env::set_var("HTTPS_PROXY", "http://proxy.invalid:3128");
    }
    let parent_path = env::var("PATH").expect("cargo runs tests with a PATH");
    let stub = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stub.sh");

    let out = run(&stub, ["env"], None, Duration::from_secs(60))
        .await
        .unwrap();

    let child: BTreeMap<&str, &str> = out
        .stdout
        .text
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    assert_eq!(child.get("PATH"), Some(&parent_path.as_str()));
    assert_eq!(child.get("TZ"), Some(&"Europe/Oslo"));
    assert_eq!(child.get("HTTPS_PROXY"), Some(&"http://proxy.invalid:3128"));
    assert!(!out.stdout.text.contains("planted"), "{}", out.stdout.text);
    assert!(!child.contains_key("TRANSCRIPTION_API_KEY"));
    assert!(!child.contains_key("MEALIE_URL"));
}

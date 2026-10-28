use std::collections::BTreeMap;
use std::env;
use std::time::Duration;

use mealie_importer::process::run;

#[path = "common/stub.rs"]
mod stub_tool;

async fn child_env() -> BTreeMap<String, String> {
    let out = run(&stub_tool::stub(), ["env"], None, Duration::from_secs(60))
        .await
        .unwrap();
    out.stdout
        .text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

#[tokio::test]
async fn the_child_gets_only_the_variables_it_needs() {
    // SAFETY: this file holds one test, so no other thread reads or writes the environment.
    unsafe {
        env::set_var("TRANSCRIPTION_API_KEY", "planted-key");
        env::set_var("MEALIE_URL", "http://planted.invalid");
        env::set_var("TZ", "Europe/Oslo");
        env::set_var("HTTPS_PROXY", "http://proxy.invalid:3128");
        env::set_var("PATH", "/usr/bin::relative/bin:/bin:.");
    }

    let child = child_env().await;

    assert_eq!(child["TZ"], "Europe/Oslo");
    assert_eq!(child["HTTPS_PROXY"], "http://proxy.invalid:3128");
    assert!(!child.values().any(|value| value.contains("planted")));
    assert!(!child.contains_key("TRANSCRIPTION_API_KEY"));
    assert!(!child.contains_key("MEALIE_URL"));
    assert_eq!(child["PATH"], "/usr/bin:/bin");

    // SAFETY: as above.
    unsafe { env::set_var("PATH", ":relative/bin:.") };

    assert!(!child_env().await.contains_key("PATH"));
}

use std::fs::File;
use std::time::Duration;

use mealie_importer::process::run;
use rustix::stdio::dup2_stdin;
use tempfile::tempfile;

#[path = "common/stub.rs"]
mod stub_tool;

#[tokio::test]
async fn the_child_reads_from_dev_null_whatever_the_parent_has_on_stdin() {
    let input: File = tempfile().unwrap();
    dup2_stdin(&input).unwrap();

    let out = run(&stub_tool::stub(), ["stdin"], None, Duration::from_secs(60))
        .await
        .unwrap();

    assert_eq!(out.stdout.text, "null\n");
}

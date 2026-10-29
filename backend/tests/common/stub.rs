use std::path::Path;

use mealie_importer::tools::{Tool, ToolHandle, ToolSet};

pub fn stub() -> ToolHandle {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    ToolSet::discover(&fixtures, None)
        .get(Tool::Ffmpeg)
        .expect("tests/fixtures/tools/bin/ffmpeg links to stub.sh")
        .clone()
}

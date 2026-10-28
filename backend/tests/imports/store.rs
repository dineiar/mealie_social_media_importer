use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mealie_importer::store::{
    DownloadDetail, ImportErrorKind, MediaKind, StepDetail, StepKey, StepState, WarningKind,
    sweep_every,
};
use serde_json::{Value, json};

use crate::support::{
    Harness, RETENTION, START, add_file, alice, at, bob, exp_and_sig, recipe, unix,
};

fn steps(states: [&str; 4]) -> Value {
    let keys = ["download", "transcribe", "create_recipe", "attach_media"];
    keys.iter()
        .zip(states)
        .map(|(key, state)| json!({ "key": key, "state": state, "detail": null }))
        .collect()
}

fn ids(imports: &[Value]) -> Vec<&str> {
    imports
        .iter()
        .map(|import| import["id"].as_str().unwrap())
        .collect()
}

#[test]
fn a_new_import_is_queued_with_four_pending_steps_and_no_expiry() {
    let harness = Harness::new();
    let writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");

    assert_eq!(
        harness.only(&alice()),
        json!({
            "id": writer.id().to_string(),
            "url": "https://www.instagram.com/reel/C9xYz12AbCd/",
            "platform": "instagram",
            "language": "pt-BR",
            "status": "queued",
            "title": null,
            "steps": steps(["pending", "pending", "pending", "pending"]),
            "error": null,
            "warnings": [],
            "recipe": null,
            "files": [],
            "created_at": START,
            "finished_at": null,
            "expires_at": null,
        })
    );
    assert!(harness.dir_of(writer.id()).join("manifest.json").is_file());
}

#[test]
fn each_write_shows_in_the_next_read() {
    let harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");

    writer.start();
    add_file(&mut writer, "video-1.mp4", &[7; 1500]);
    add_file(&mut writer, "transcript.txt", b"first try");
    add_file(&mut writer, "transcript.txt", b"Okay, so start with butter");
    let download = DownloadDetail {
        media: MediaKind::Video,
        bytes: 1500,
        duration_s: Some(48),
    };
    writer.set_step(
        StepKey::Download,
        StepState::Done,
        Some(StepDetail::Download(download)),
    );
    writer.set_step(StepKey::Transcribe, StepState::Active, None);
    writer.warn(WarningKind::TranscriptionFailed);
    writer.warn(WarningKind::TranscriptionFailed);
    writer.set_title("One-pan lemon orzo".to_owned());

    let import = harness.only(&alice());
    assert_eq!(import["status"], "running");
    assert_eq!(import["title"], "One-pan lemon orzo");
    assert_eq!(
        import["steps"][0],
        json!({
            "key": "download",
            "state": "done",
            "detail": { "media": "video", "bytes": 1500, "duration_s": 48 },
        })
    );
    assert_eq!(
        import["steps"][1],
        json!({ "key": "transcribe", "state": "active", "detail": null })
    );
    assert_eq!(
        import["warnings"],
        json!([{ "kind": "transcription_failed", "message": "The audio could not be transcribed." }])
    );
    let files = import["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "re-adding a file replaces it: {files:#?}");
    assert_eq!(
        (&files[0]["kind"], &files[0]["name"]),
        (&json!("video"), &json!("video-1.mp4"))
    );
    assert_eq!(
        (&files[0]["mime"], &files[0]["bytes"]),
        (&json!("video/mp4"), &json!(1500))
    );
    assert_eq!(
        (&files[1]["kind"], &files[1]["mime"], &files[1]["bytes"]),
        (&json!("text"), &json!("text/plain"), &json!(26))
    );
    let url = files[0]["url"].as_str().unwrap();
    let (path, _) = url.split_once('?').unwrap();
    assert_eq!(path, format!("/api/files/{}/video-1.mp4", writer.id()));
    let (exp, sig) = exp_and_sig(url);
    assert_eq!(
        exp.parse::<i64>().unwrap(),
        unix(START) + 3600,
        "a running import's link lasts one retention period from the poll"
    );
    assert_eq!(sig.len(), 64);
    assert!(sig.bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn a_succeeded_import_carries_the_recipe_and_expires_one_retention_after_it_finished() {
    let harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/p/C9xYz12AbCd/");
    let id = writer.id();
    add_file(&mut writer, "cover.jpg", b"jpeg");
    harness.clock.advance(Duration::from_secs(30));
    writer.succeed(recipe());

    let import = harness.only(&alice());
    assert_eq!(import["status"], "succeeded");
    assert_eq!(import["error"], Value::Null);
    assert_eq!(
        import["recipe"],
        json!({
            "slug": "one-pan-lemon-orzo",
            "name": "One-pan lemon orzo",
            "url": "https://recipes.example.com/g/home/r/one-pan-lemon-orzo",
        })
    );
    assert_eq!(import["finished_at"], at("2026-10-20T14:02:41Z"));
    assert_eq!(import["expires_at"], at("2026-10-20T15:02:41Z"));
    let url = import["files"][0]["url"].as_str().unwrap();
    assert!(
        url.contains(&format!("exp={}&", unix("2026-10-20T15:02:41Z"))),
        "a finished import's link expires with it: {url}"
    );

    harness.clock.advance(RETENTION - Duration::from_secs(1));
    assert!(harness.store.get(&alice(), id).is_some(), "one second left");

    harness.clock.advance(Duration::from_secs(1));
    assert!(harness.store.list(&alice()).is_empty());
    assert!(harness.store.get(&alice(), id).is_none());
}

#[test]
fn a_failed_import_carries_the_error_and_no_recipe() {
    let harness = Harness::new();
    let writer = harness.create(&alice(), "https://www.instagram.com/p/C9xYz12AbCd/");
    writer.fail(ImportErrorKind::MealieUnreachable);

    let import = harness.only(&alice());
    assert_eq!(import["status"], "failed");
    assert_eq!(
        import["error"],
        json!({ "kind": "mealie_unreachable", "message": "Mealie did not respond." })
    );
    assert_eq!(import["recipe"], Value::Null);
    assert_eq!(import["finished_at"], START);
    assert_eq!(import["expires_at"], at("2026-10-20T15:02:11Z"));
}

#[test]
fn each_user_sees_only_their_own_imports_newest_first() {
    let harness = Harness::new();
    let first = harness.create(&alice(), "https://www.instagram.com/p/first/");
    let other = harness.create(&bob(), "https://www.instagram.com/p/other/");
    let second = harness.create(&alice(), "https://www.instagram.com/p/second/");
    harness.clock.advance(Duration::from_secs(5));
    let third = harness.create(&alice(), "https://www.instagram.com/p/third/");

    let theirs = harness.list(&alice());
    assert_eq!(
        ids(&theirs),
        [third.id(), second.id(), first.id()].map(|id| id.to_string()),
        "newest first, also within one second"
    );
    assert_eq!(ids(&harness.list(&bob())), [other.id().to_string()]);
    assert!(harness.store.get(&bob(), first.id()).is_none());
    assert!(harness.store.get(&alice(), other.id()).is_none());
    assert!(harness.store.get(&alice(), first.id()).is_some());
}

#[test]
fn dropping_a_writer_before_it_finishes_fails_the_import_as_interrupted() {
    let mut harness = Harness::new();
    let mut writer = harness.create(&alice(), "https://www.instagram.com/reel/C9xYz12AbCd/");
    writer.start();
    writer.set_step(StepKey::Download, StepState::Active, None);
    harness.clock.advance(Duration::from_secs(9));
    drop(writer);

    let import = harness.only(&alice());
    assert_eq!(import["status"], "failed");
    assert_eq!(import["error"]["kind"], "interrupted");
    assert_eq!(import["finished_at"], at("2026-10-20T14:02:20Z"));
    assert_eq!(
        import["steps"],
        steps(["failed", "pending", "pending", "pending"])
    );

    harness.clock.advance(Duration::from_secs(60));
    harness.restart();
    assert_eq!(
        harness.only(&alice()),
        import,
        "the manifest held the interrupted state"
    );
}

#[test]
fn a_restart_fails_unfinished_imports_as_interrupted_and_keeps_finished_ones() {
    let mut harness = Harness::new();
    let queued = harness.create(&alice(), "https://www.instagram.com/p/queued/");
    let mut running = harness.create(&alice(), "https://www.instagram.com/reel/running/");
    running.start();
    add_file(&mut running, "video-1.mp4", b"mp4");
    running.set_step(StepKey::Download, StepState::Done, None);
    running.set_step(StepKey::Transcribe, StepState::Active, None);
    let succeeded = harness.create(&alice(), "https://www.instagram.com/p/succeeded/");
    succeeded.succeed(recipe());
    let failed = harness.create(&bob(), "https://www.instagram.com/p/failed/");
    failed.fail(ImportErrorKind::NoRecipe);
    let before = harness.list(&alice());
    let bobs_before = harness.only(&bob());
    let (queued_id, running_id) = (queued.id(), running.id());
    std::mem::forget(queued);
    std::mem::forget(running);

    harness.clock.advance(Duration::from_mins(10));
    harness.restart();

    let after = harness.list(&alice());
    assert_eq!(ids(&after), ids(&before), "the order survives a restart");
    assert_eq!(after[0], before[0], "a succeeded import is unchanged");
    assert_eq!(
        harness.only(&bob()),
        bobs_before,
        "a failed import is unchanged"
    );
    for (import, id) in [(&after[1], running_id), (&after[2], queued_id)] {
        assert_eq!(import["id"], id.to_string());
        assert_eq!(import["status"], "failed");
        assert_eq!(
            import["error"],
            json!({ "kind": "interrupted", "message": "The import stopped before it finished." })
        );
        assert_eq!(import["finished_at"], at("2026-10-20T14:12:11Z"));
        assert_eq!(import["expires_at"], at("2026-10-20T15:12:11Z"));
    }
    assert_eq!(
        after[1]["steps"],
        steps(["done", "failed", "pending", "pending"])
    );
    assert_eq!(after[1]["files"][0]["name"], "video-1.mp4");
    assert_eq!(
        after[2]["steps"],
        steps(["pending", "pending", "pending", "pending"])
    );

    harness.clock.advance(Duration::from_mins(10));
    harness.restart();
    assert_eq!(
        harness.list(&alice())[1]["finished_at"],
        at("2026-10-20T14:12:11Z"),
        "a second restart keeps the first restart's finish time"
    );
}

#[test]
fn a_restart_deletes_expired_imports_and_unparseable_manifests_and_nothing_else() {
    let mut harness = Harness::new();
    let expired = harness.create(&alice(), "https://www.instagram.com/p/expired/");
    let expired_id = expired.id();
    expired.fail(ImportErrorKind::DownloadFailed);
    harness.clock.advance(Duration::from_mins(30));
    let live = harness.create(&alice(), "https://www.instagram.com/p/live/");
    let live_id = live.id();
    live.fail(ImportErrorKind::DownloadFailed);

    let imports = harness.imports_dir();
    let garbage = imports.join("11111111-1111-4111-8111-111111111111");
    fs::create_dir(&garbage).unwrap();
    fs::write(garbage.join("manifest.json"), "{ not json").unwrap();
    let empty = imports.join("22222222-2222-4222-8222-222222222222");
    fs::create_dir(&empty).unwrap();
    let moved = imports.join("33333333-3333-4333-8333-333333333333");
    fs::create_dir(&moved).unwrap();
    fs::copy(
        harness.dir_of(live_id).join("manifest.json"),
        moved.join("manifest.json"),
    )
    .unwrap();
    let foreign = imports.join("keep-me");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("notes.txt"), "mine").unwrap();
    fs::write(imports.join("README.txt"), "mine").unwrap();

    harness.clock.advance(Duration::from_mins(30));
    harness.restart();

    assert_eq!(ids(&harness.list(&alice())), [live_id.to_string()]);
    for gone in [harness.dir_of(expired_id), garbage, empty, moved] {
        assert!(!gone.exists(), "{} is still there", gone.display());
    }
    assert!(harness.dir_of(live_id).join("manifest.json").is_file());
    assert!(foreign.join("notes.txt").is_file());
    assert!(imports.join("README.txt").is_file());
}

#[test]
fn the_sweeper_deletes_expired_imports_and_only_them() {
    let harness = Harness::new();
    let mut done = harness.create(&alice(), "https://www.instagram.com/p/done/");
    add_file(&mut done, "cover.jpg", b"jpeg");
    let done_id = done.id();
    done.succeed(recipe());
    let mut running = harness.create(&alice(), "https://www.instagram.com/reel/running/");
    running.start();
    let cookies = harness.data_dir.path().join("cookies");
    fs::create_dir(&cookies).unwrap();
    fs::write(cookies.join("instagram.txt"), "# Netscape HTTP Cookie File").unwrap();
    fs::write(harness.imports_dir().join("README.txt"), "mine").unwrap();

    harness.clock.advance(RETENTION - Duration::from_secs(1));
    harness.store.sweep();
    assert!(harness.dir_of(done_id).is_dir(), "not expired yet");

    harness.clock.advance(Duration::from_secs(1));
    harness.store.sweep();
    assert!(!harness.dir_of(done_id).exists());
    assert_eq!(ids(&harness.list(&alice())), [running.id().to_string()]);
    assert!(harness.dir_of(running.id()).join("manifest.json").is_file());
    assert!(cookies.join("instagram.txt").is_file());
    assert!(harness.imports_dir().join("README.txt").is_file());

    harness.clock.advance(Duration::from_hours(24));
    harness.store.sweep();
    assert!(
        harness.dir_of(running.id()).is_dir(),
        "a running import never expires"
    );
}

#[tokio::test]
async fn sweep_every_sweeps_on_its_period() {
    let harness = Harness::new();
    let done = harness.create(&alice(), "https://www.instagram.com/p/done/");
    let dir = harness.dir_of(done.id());
    done.fail(ImportErrorKind::NoRecipe);
    harness.clock.advance(RETENTION);
    let sweeper = tokio::spawn(sweep_every(
        Arc::clone(&harness.store),
        Duration::from_millis(20),
    ));

    let deadline = Instant::now() + Duration::from_secs(5);
    while dir.exists() {
        assert!(Instant::now() < deadline, "the sweeper never ran");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    sweeper.abort();
}

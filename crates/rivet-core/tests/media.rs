use rivet_core::{DownloadRequest, media, native, queue::Queue};
use tempfile::TempDir;

#[test]
fn page_validation_limits_media_to_one_public_youtube_id() {
    for url in [
        "https://youtu.be/aBbaea3I5bo?t=1",
        "https://www.youtube.com/watch?v=aBbaea3I5bo&list=ignored",
        "https://m.youtube.com/shorts/aBbaea3I5bo",
    ] {
        assert_eq!(
            media::youtube_page(url).unwrap(),
            "https://www.youtube.com/watch?v=aBbaea3I5bo"
        );
    }
    for url in [
        "http://youtube.com/watch?v=aBbaea3I5bo",
        "https://youtube.com.evil.test/watch?v=aBbaea3I5bo",
        "https://youtube.com/playlist?list=all",
        "https://user:secret@youtube.com/watch?v=aBbaea3I5bo",
        "https://youtube.com/watch?v=--help",
        "https://youtube.com:8443/watch?v=aBbaea3I5bo",
    ] {
        assert!(media::youtube_page(url).is_err(), "{url}");
    }
}
#[test]
fn fixed_arguments_disable_configuration_plugins_and_remote_code() {
    let dir = TempDir::new().unwrap();
    let request = DownloadRequest::new(
        "https://youtu.be/aBbaea3I5bo".into(),
        dir.path().join("final.mp4"),
    );
    let arguments = media::arguments(
        &request,
        &dir.path().join("100% folder"),
        &dir.path().join("ffmpeg"),
        &dir.path().join("qjs"),
    )
    .unwrap();
    let values: Vec<_> = arguments.iter().map(|a| a.to_str().unwrap()).collect();
    for required in [
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-playlist",
        "--keep-video",
        "--no-js-runtimes",
    ] {
        assert!(values.contains(&required));
    }
    assert!(values.iter().any(|a| a.contains("100%% folder")));
    assert_eq!(values[values.len() - 2], "--");
    assert_eq!(
        values.last().unwrap(),
        &"https://www.youtube.com/watch?v=aBbaea3I5bo"
    );
    assert!(!values.contains(&"--cookies-from-browser"));
}
fn frame(value: serde_json::Value) -> Vec<u8> {
    let data = serde_json::to_vec(&value).unwrap();
    let mut out = (data.len() as u32).to_le_bytes().to_vec();
    out.extend(data);
    out
}
#[test]
fn typed_media_jobs_start_only_after_successful_desktop_action() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("queue.db");
    let queue = Queue::open(&db).unwrap();
    let input = frame(serde_json::json!({"type":"media","url":"https://youtu.be/aBbaea3I5bo"}));
    let mut output = Vec::new();
    native::serve_with_actions(
        &mut input.as_slice(),
        &mut output,
        &queue,
        dir.path(),
        || {},
        || anyhow::bail!("desktop unavailable"),
    )
    .unwrap();
    assert!(queue.list().unwrap().is_empty());
    native::serve_with_actions(
        &mut input.as_slice(),
        &mut Vec::new(),
        &queue,
        dir.path(),
        || {},
        || Ok(()),
    )
    .unwrap();
    let jobs = queue.list().unwrap();
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].media);
    assert!(jobs[0].start_requested);
    assert!(jobs[0].request.destination.starts_with(dir.path()));
    assert!(queue.claim(&jobs[0].id, "test").unwrap());
    assert!(!queue.get(&jobs[0].id).unwrap().start_requested);
}
#[test]
fn ordinary_queue_jobs_remain_manual_and_stop_all_clears_pending_actions() {
    let dir = TempDir::new().unwrap();
    let queue = Queue::open(dir.path().join("queue.db")).unwrap();
    let request = DownloadRequest::new("https://example.com/file".into(), dir.path().join("file"));
    let id = queue.enqueue(request.clone(), 0).unwrap();
    assert!(!queue.get(&id).unwrap().start_requested);
    assert!(!queue.get(&id).unwrap().media);
    let immediate = queue.enqueue_download(request).unwrap();
    assert!(queue.get(&immediate).unwrap().start_requested);
    queue.clear_start_requests().unwrap();
    assert!(!queue.get(&immediate).unwrap().start_requested);
    let reopened = Queue::open(dir.path().join("queue.db")).unwrap();
    assert_eq!(reopened.list().unwrap().len(), 2);
}

/// Explicit opt-in: exercises packaged resolver/process cancellation against a real public video.
#[tokio::test]
#[ignore = "Requires RIVET_LIVE_MEDIA_URL, prepared media tools and network access"]
async fn live_media_pause_and_resume_preserves_a_complete_video() {
    use rivet_core::{CancellationToken, Engine, Progress, worker};
    use std::time::Duration;
    let url = std::env::var("RIVET_LIVE_MEDIA_URL").expect("Set a public completed YouTube URL");
    let dir = TempDir::new().unwrap();
    let destination = dir.path().join("result.mp4");
    let queue = Queue::open(dir.path().join("queue.db")).unwrap();
    let id = queue
        .enqueue_media(DownloadRequest::new(url, destination.clone()))
        .unwrap();
    let progress = Progress::default();
    let cancel = CancellationToken::new();
    let run = tokio::spawn({
        let queue = queue.clone();
        let id = id.clone();
        let cancel = cancel.clone();
        let progress = progress.clone();
        async move { worker::run_job(queue, &id, Engine::new(None).unwrap(), cancel, progress).await }
    });
    tokio::time::timeout(Duration::from_secs(90), async {
        while progress.snapshot().0 < 32768 {
            assert!(!run.is_finished(), "Download ended before cancellation");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(15), run)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(queue.get(&id).unwrap().status, "paused");
    assert!(!destination.exists());
    assert!(
        dir.path()
            .join(".result.mp4.rivet-media/source.json")
            .is_file()
    );
    queue.retry(&id).unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(180),
        worker::run_job(
            queue.clone(),
            &id,
            Engine::new(None).unwrap(),
            CancellationToken::new(),
            Progress::default(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(result.bytes > 0);
    assert_eq!(
        queue.get(&id).unwrap().request.destination,
        result.destination
    );
    assert_ne!(result.destination, destination);
    assert_eq!(queue.get(&id).unwrap().status, "complete");
    let probe = tokio::process::Command::new(media::tool("ffprobe").unwrap())
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "json",
        ])
        .arg(&result.destination)
        .output()
        .await
        .unwrap();
    let tracks: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    for required in ["video", "audio"] {
        assert!(
            tracks["streams"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["codec_type"] == required)
        );
    }
}

use rivet_core::{DownloadRequest, DownloadResult, filenames, queue::Queue};
use tempfile::TempDir;
#[test]
fn names_keep_unicode_and_remove_cross_platform_path_and_device_syntax() {
    assert_eq!(
        filenames::safe_name("My résumé 日本語.mp4"),
        "My résumé 日本語.mp4"
    );
    for raw in [
        "../escape",
        "CON.mp4",
        "a/b\\c:?.mp4",
        "name. ",
        "\nfile.mp4",
    ] {
        let safe = filenames::safe_name(raw);
        assert!(!safe.starts_with('.'));
        assert!(!safe.contains(['/', '\\', ':', '?', '\n']));
        assert!(!safe.ends_with(['.', ' ']));
    }
    assert_eq!(filenames::safe_name("CON.mp4"), "_CON.mp4");
    assert!(filenames::safe_name(&"界".repeat(200)).len() <= 180);
}
#[test]
fn atomic_publication_preserves_existing_files_and_numbers_duplicates() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("stream");
    std::fs::write(&source, b"new video").unwrap();
    let original = temp.path().join("Video.mp4");
    std::fs::write(&original, b"existing video").unwrap();
    let result = filenames::publish(&source, temp.path(), "Video.mp4").unwrap();
    assert_eq!(result.file_name().unwrap(), "Video (2).mp4");
    assert_eq!(std::fs::read(&original).unwrap(), b"existing video");
    assert_eq!(std::fs::read(result).unwrap(), b"new video");
}
#[test]
fn final_filename_is_durable_and_only_the_owning_worker_can_commit_it() {
    let temp = TempDir::new().unwrap();
    let queue = Queue::open(temp.path().join("queue.db")).unwrap();
    let old = temp.path().join("placeholder.mp4");
    let id = queue
        .enqueue_media(DownloadRequest::new(
            "https://youtu.be/aBbaea3I5bo".into(),
            old,
        ))
        .unwrap();
    queue.claim(&id, "owner").unwrap();
    let done = DownloadResult {
        destination: temp.path().join("Video title.mp4"),
        bytes: 10,
        sha256: "a".repeat(64),
    };
    assert!(queue.finish_download(&id, "other", &done).is_err());
    queue.finish_download(&id, "owner", &done).unwrap();
    let reopened = Queue::open(temp.path().join("queue.db")).unwrap();
    let job = reopened.get(&id).unwrap();
    assert_eq!(job.status, "complete");
    assert_eq!(job.request.destination, done.destination);
    assert_eq!(job.bytes, 10);
}
#[test]
fn rename_keeps_contents_and_history_and_refuses_overwrite_or_parent_traversal() {
    let temp = TempDir::new().unwrap();
    let queue = Queue::open(temp.path().join("queue.db")).unwrap();
    let old = temp.path().join("old.mp4");
    std::fs::write(&old, b"complete video").unwrap();
    let id = queue
        .enqueue(
            DownloadRequest::new("https://example.com/video".into(), old.clone()),
            0,
        )
        .unwrap();
    assert!(queue.rename_completed(&id, "new.mp4").is_err());
    queue.claim(&id, "owner").unwrap();
    queue
        .finish(&id, "owner", "complete", 14, Some(&"a".repeat(64)), None)
        .unwrap();
    std::fs::write(temp.path().join("exists.mp4"), b"keep").unwrap();
    for name in ["exists.mp4", "../escape.mp4", "a/b.mp4", "NUL.mp4"] {
        assert!(queue.rename_completed(&id, name).is_err());
        assert!(old.exists());
    }
    queue.rename_completed(&id, "Renamed vidéo.mp4").unwrap();
    let new = temp.path().join("Renamed vidéo.mp4");
    assert_eq!(std::fs::read(&new).unwrap(), b"complete video");
    assert!(!old.exists());
    assert_eq!(queue.get(&id).unwrap().request.destination, new);
}

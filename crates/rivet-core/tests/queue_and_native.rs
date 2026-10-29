use rivet_core::{
    DownloadRequest,
    native::{self, CaptureReply},
    queue::{Queue, now},
};
use tempfile::TempDir;

#[test]
fn durable_queue_enforces_worker_ownership_and_schedule() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("queue.sqlite3");
    let queue = Queue::open(&path).unwrap();
    let id = queue
        .enqueue(
            DownloadRequest::new("https://example.com/file".into(), dir.path().join("file")),
            0,
        )
        .unwrap();
    let second = Queue::open(&path).unwrap();
    assert!(queue.claim(&id, "worker-a").unwrap());
    assert!(!second.claim(&id, "worker-b").unwrap());
    assert!(second.remove(&id).is_err());
    assert!(
        second
            .finish(&id, "worker-b", "complete", 1, None, None)
            .is_err()
    );
    queue
        .finish(&id, "worker-a", "paused", 100, None, None)
        .unwrap();
    second.retry(&id).unwrap();
    assert_eq!(queue.next_queued().unwrap(), Some(id.clone()));
    assert_eq!(queue.get(&id).unwrap().bytes, 100);
    queue.remove(&id).unwrap();
    let later = queue
        .enqueue(
            DownloadRequest::new("https://example.com/later".into(), dir.path().join("later")),
            now() + 3600,
        )
        .unwrap();
    assert!(queue.next_queued().unwrap().is_none());
    assert!(!queue.claim(&later, "worker-a").unwrap());
}

fn frame(json: &[u8]) -> Vec<u8> {
    let mut out = (json.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(json);
    out
}

#[test]
fn browser_capture_queues_without_starting_or_accepting_paths() {
    let dir = TempDir::new().unwrap();
    let queue = Queue::open(dir.path().join("queue.sqlite3")).unwrap();
    let downloads = dir.path().join("downloads");
    let input = frame(br#"{"url":"https://example.com/../../evil.exe?token=private"}"#);
    let mut output = Vec::new();
    native::serve(&mut input.as_slice(), &mut output, &queue, &downloads).unwrap();
    let jobs = queue.list().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "queued");
    assert_eq!(jobs[0].request.destination.parent().unwrap(), downloads);
    assert!(!jobs[0].request.destination.exists());
    assert!(
        !jobs[0]
            .request
            .destination
            .to_string_lossy()
            .contains("token")
    );
    let reply: serde_json::Value = serde_json::from_slice(&output[4..]).unwrap();
    assert_eq!(reply["ok"], true);
}

#[test]
fn native_framing_rejects_oversize_truncation_and_extra_fields() {
    assert!(
        native::read_frame(&mut (native::MAX_MESSAGE as u32 + 1).to_le_bytes().as_slice()).is_err()
    );
    assert!(native::read_frame(&mut [5_u8, 0, 0, 0, b'{'].as_slice()).is_err());
    assert!(native::read_frame(&mut [5_u8, 0].as_slice()).is_err());
    assert!(
        native::read_frame(
            &mut frame(br#"{"url":"https://example.com","destination":"/tmp/unwanted"}"#)
                .as_slice()
        )
        .is_err()
    );
    assert!(native::read_frame(&mut [].as_slice()).unwrap().is_none());
    let mut data = Vec::new();
    native::write_frame(
        &mut data,
        &CaptureReply {
            ok: true,
            id: Some("job".into()),
            error: None,
        },
    )
    .unwrap();
    assert_eq!(
        u32::from_le_bytes(data[..4].try_into().unwrap()) as usize,
        data.len() - 4
    );
}

#[test]
fn browser_cannot_queue_file_urls_or_invalid_connections() {
    let dir = TempDir::new().unwrap();
    let queue = Queue::open(dir.path().join("queue.sqlite3")).unwrap();
    assert!(
        queue
            .enqueue(
                DownloadRequest::new("file:///etc/passwd".into(), dir.path().join("out")),
                0
            )
            .is_err()
    );
    let mut request = DownloadRequest::new("https://example.com".into(), dir.path().join("out"));
    request.connections = 0;
    assert!(queue.enqueue(request, 0).is_err());
}

#[test]
fn queue_metadata_progress_and_schedule_survive_reopening() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("queue.sqlite3");
    let q = Queue::open(&path).unwrap();
    let id = q
        .enqueue(
            DownloadRequest::new(
                "https://example.com/video.mp4".into(),
                dir.path().join("video.mp4"),
            ),
            0,
        )
        .unwrap();
    q.set_metadata(&id, "Video", "Review footage").unwrap();
    assert!(q.claim(&id, "worker").unwrap());
    q.heartbeat(&id, "worker", 128, 1024).unwrap();
    assert!(q.schedule(&id, now() + 60).is_err());
    q.finish(&id, "worker", "paused", 128, None, None).unwrap();
    q.schedule(&id, now() + 3600).unwrap();
    let saved = Queue::open(path).unwrap().get(&id).unwrap();
    assert_eq!(saved.category, "Video");
    assert_eq!(saved.description, "Review footage");
    assert_eq!(saved.total_bytes, 1024);
    assert!(saved.last_try > 0);
    assert_eq!(saved.status, "queued");
    assert!(saved.not_before > now());
}

#[test]
fn opening_the_original_schema_preserves_existing_jobs() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("legacy.sqlite3");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE jobs(id TEXT PRIMARY KEY,request TEXT NOT NULL,status TEXT NOT NULL,bytes INTEGER NOT NULL DEFAULT 0,sha256 TEXT,error TEXT,created INTEGER NOT NULL,not_before INTEGER NOT NULL,owner TEXT,lease INTEGER NOT NULL DEFAULT 0);").unwrap();
    let request = DownloadRequest::new("https://example.com/file".into(), dir.path().join("file"));
    conn.execute(
        "INSERT INTO jobs(id,request,status,created,not_before) VALUES('legacy',?1,'queued',?2,0)",
        rusqlite::params![serde_json::to_string(&request).unwrap(), now()],
    )
    .unwrap();
    drop(conn);
    let migrated = Queue::open(path).unwrap().get("legacy").unwrap();
    assert_eq!(migrated.request.url, request.url);
    assert_eq!(migrated.description, "");
    assert_eq!(migrated.category, "auto");
    assert_eq!(migrated.total_bytes, 0);
}

#[test]
fn connection_probe_never_queues_or_creates_a_download_folder() {
    let dir = TempDir::new().unwrap();
    let queue = Queue::open(dir.path().join("queue.sqlite3")).unwrap();
    let downloads = dir.path().join("downloads");
    let mut output = Vec::new();
    native::serve(
        &mut frame(br#"{"type":"ping"}"#).as_slice(),
        &mut output,
        &queue,
        &downloads,
    )
    .unwrap();
    let reply: serde_json::Value = serde_json::from_slice(&output[4..]).unwrap();
    assert_eq!(reply["ok"], true);
    assert!(reply["id"].is_null());
    assert!(queue.list().unwrap().is_empty());
    assert!(!downloads.exists());
    for invalid in [
        br#"{"type":"ping","url":"https://example.com"}"#.as_slice(),
        br#"{"type":"execute"}"#.as_slice(),
    ] {
        assert!(native::read_frame(&mut frame(invalid).as_slice()).is_err());
    }
}

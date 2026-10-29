use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderMap, Response, StatusCode},
    routing::get,
};
use futures_util::stream;
use rivet_core::{CancellationToken, DownloadRequest, Engine, Progress};
use sha2::{Digest, Sha256};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;

#[derive(Clone, Copy)]
enum Mode {
    Ranges,
    IgnoreRanges,
    WrongRange,
    Encoded,
    TruncateOnce,
    NoValidator,
    Empty,
    UnknownSize,
    Overlong,
    AbortAfterBody,
}
#[derive(Clone)]
struct Fixture {
    data: Arc<Vec<u8>>,
    mode: Mode,
    version: Arc<AtomicUsize>,
    transfers: Arc<AtomicUsize>,
}
struct Server {
    url: String,
    fixture: Fixture,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(mode: Mode) -> Server {
    let fixture = Fixture {
        data: Arc::new((0..1024 * 1024).map(|n| (n % 251) as u8).collect()),
        mode,
        version: Arc::new(AtomicUsize::new(1)),
        transfers: Arc::new(AtomicUsize::new(0)),
    };
    let router = Router::new()
        .route("/file", get(handler))
        .with_state(fixture.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Server { url, fixture, task }
}

async fn handler(State(f): State<Fixture>, headers: HeaderMap) -> Response<Body> {
    if matches!(f.mode, Mode::Empty) {
        return if headers.contains_key("range") {
            Response::builder()
                .status(416)
                .header("content-range", "bytes */0")
                .body(Body::empty())
                .unwrap()
        } else {
            Response::builder()
                .header("content-length", 0)
                .body(Body::empty())
                .unwrap()
        };
    }
    let range = headers
        .get("range")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("bytes="))
        .and_then(|s| s.split_once('-'));
    let total = f.data.len();
    let (start, end, ranged) = if !matches!(f.mode, Mode::IgnoreRanges | Mode::UnknownSize) {
        range
            .map(|(a, b)| {
                (
                    a.parse::<usize>().unwrap(),
                    b.parse::<usize>().unwrap(),
                    true,
                )
            })
            .unwrap_or((0, total - 1, false))
    } else {
        (0, total - 1, false)
    };
    let probe = ranged && start == 0 && end == 0;
    let truncated = !probe
        && matches!(f.mode, Mode::TruncateOnce)
        && f.transfers.fetch_add(1, Ordering::SeqCst) == 0;
    let data = f.data[start..=if truncated {
        start + (end - start) / 2
    } else {
        end
    }]
        .to_vec();
    let overlong = !probe && matches!(f.mode, Mode::Overlong);
    let mut chunks: Vec<_> = data.chunks(8192).map(Bytes::copy_from_slice).collect();
    if overlong {
        chunks.push(Bytes::from_static(b"unexpected extra bytes"));
    }
    let abort_after_body = !probe && matches!(f.mode, Mode::AbortAfterBody);
    let body = Body::from_stream(stream::unfold((chunks, 0), move |(chunks, i)| async move {
        if i == chunks.len() && abort_after_body {
            return Some((
                Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "fixture body failure before clean EOF",
                )),
                (chunks, i + 1),
            ));
        }
        if i >= chunks.len() {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
        let chunk = chunks[i].clone();
        Some((Ok::<_, std::io::Error>(chunk), (chunks, i + 1)))
    }));
    let mut response = Response::builder().status(if ranged {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    });
    if !matches!(f.mode, Mode::NoValidator) {
        response = response.header("etag", format!("\"v{}\"", f.version.load(Ordering::SeqCst)));
    }
    if ranged {
        let actual_start = if !probe && matches!(f.mode, Mode::WrongRange) {
            start + 1
        } else {
            start
        };
        response = response.header(
            "content-range",
            format!("bytes {actual_start}-{end}/{total}"),
        );
    }
    if !truncated && !overlong && !abort_after_body && !matches!(f.mode, Mode::UnknownSize) {
        response = response.header("content-length", end - start + 1);
    }
    if matches!(f.mode, Mode::Encoded) {
        response = response.header("content-encoding", "gzip");
    }
    response.body(body).unwrap()
}

fn request(server: &Server, temp: &TempDir) -> DownloadRequest {
    DownloadRequest::new(server.url.clone(), temp.path().join("download.bin"))
}

#[tokio::test]
async fn segmented_download_is_byte_exact_and_never_overwrites() {
    let server = serve(Mode::Ranges).await;
    let temp = TempDir::new().unwrap();
    let mut r = request(&server, &temp);
    r.expected_sha256 = Some(format!(
        "{:x}",
        Sha256::digest(server.fixture.data.as_slice())
    ));
    let engine = Engine::new(None).unwrap();
    let result = engine
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&r.destination).unwrap(), *server.fixture.data);
    assert_eq!(result.bytes, 1024 * 1024);
    assert!(
        engine
            .download(r, CancellationToken::new(), Progress::default())
            .await
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
}

#[tokio::test]
async fn cancelled_ranges_resume_without_corruption() {
    let server = serve(Mode::Ranges).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    let engine = Engine::new(None).unwrap();
    let progress = Progress::default();
    let token = CancellationToken::new();
    let (e, rr, p, t) = (engine.clone(), r.clone(), progress.clone(), token.clone());
    let task = tokio::spawn(async move { e.download(rr, t, p).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while progress.snapshot().0 < 65536 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    token.cancel();
    assert!(task.await.unwrap().is_err());
    assert!(!r.destination.exists());
    engine
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap();
    assert_eq!(std::fs::read(r.destination).unwrap(), *server.fixture.data);
}

#[tokio::test]
async fn changed_resource_is_not_spliced_into_old_parts() {
    let server = serve(Mode::Ranges).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    let engine = Engine::new(None).unwrap();
    // A deliberately wrong expected hash leaves complete verified range parts unpublished.
    let mut wrong = r.clone();
    wrong.expected_sha256 = Some("0".repeat(64));
    assert!(
        engine
            .download(wrong, CancellationToken::new(), Progress::default())
            .await
            .is_err()
    );
    server.fixture.version.store(2, Ordering::SeqCst);
    let error = engine
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed"));
    assert!(!r.destination.exists());
}

#[tokio::test]
async fn ignored_ranges_fall_back_to_a_single_stream() {
    let server = serve(Mode::IgnoreRanges).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    Engine::new(None)
        .unwrap()
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap();
    assert_eq!(std::fs::read(r.destination).unwrap(), *server.fixture.data);
}

#[tokio::test]
async fn no_validator_uses_single_stream() {
    let server = serve(Mode::NoValidator).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    Engine::new(None)
        .unwrap()
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap();
    assert_eq!(std::fs::read(r.destination).unwrap(), *server.fixture.data);
}

#[tokio::test]
async fn mismatched_ranges_never_publish_an_output() {
    let server = serve(Mode::WrongRange).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    assert!(
        Engine::new(None)
            .unwrap()
            .download(r.clone(), CancellationToken::new(), Progress::default())
            .await
            .is_err()
    );
    assert!(!r.destination.exists());
}

#[tokio::test]
async fn compression_cannot_invalidate_range_offsets() {
    let server = serve(Mode::Encoded).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    assert!(
        Engine::new(None)
            .unwrap()
            .download(r.clone(), CancellationToken::new(), Progress::default())
            .await
            .is_err()
    );
    assert!(!r.destination.exists());
}

#[tokio::test]
async fn short_response_retries_the_missing_tail() {
    let server = serve(Mode::TruncateOnce).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    Engine::new(None)
        .unwrap()
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap();
    assert_eq!(std::fs::read(r.destination).unwrap(), *server.fixture.data);
}

#[tokio::test]
async fn failed_checksum_keeps_destination_unpublished() {
    let server = serve(Mode::Ranges).await;
    let temp = TempDir::new().unwrap();
    let mut r = request(&server, &temp);
    r.expected_sha256 = Some("0".repeat(64));
    let error = Engine::new(None)
        .unwrap()
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("SHA-256 mismatch"));
    assert!(!r.destination.exists());
}

#[tokio::test]
async fn empty_resource_is_a_valid_download() {
    let server = serve(Mode::Empty).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    let done = Engine::new(None)
        .unwrap()
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap();
    assert_eq!(done.bytes, 0);
    assert_eq!(std::fs::read(r.destination).unwrap(), Vec::<u8>::new());
}

#[tokio::test]
async fn unknown_length_stream_is_supported() {
    let server = serve(Mode::UnknownSize).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    Engine::new(None)
        .unwrap()
        .download(r.clone(), CancellationToken::new(), Progress::default())
        .await
        .unwrap();
    assert_eq!(std::fs::read(r.destination).unwrap(), *server.fixture.data);
}

#[tokio::test]
async fn overlong_response_is_not_accepted_on_retry() {
    let server = serve(Mode::Overlong).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    assert!(
        Engine::new(None)
            .unwrap()
            .download(r.clone(), CancellationToken::new(), Progress::default())
            .await
            .is_err()
    );
    assert!(!r.destination.exists());
}

#[tokio::test]
async fn concurrent_writers_cannot_share_a_destination() {
    let server = serve(Mode::Ranges).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    let progress = Progress::default();
    let (rr, p) = (r.clone(), progress.clone());
    let engine = Engine::new(None).unwrap();
    let e = engine.clone();
    let first = tokio::spawn(async move { e.download(rr, CancellationToken::new(), p).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while progress.snapshot().0 < 32768 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    let error = engine
        .download(r, CancellationToken::new(), Progress::default())
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("owns this destination"));
    first.await.unwrap().unwrap();
}

#[tokio::test]
async fn incomplete_response_framing_cannot_leave_a_completed_part_on_retry() {
    let server = serve(Mode::AbortAfterBody).await;
    let temp = TempDir::new().unwrap();
    let r = request(&server, &temp);
    assert!(
        Engine::new(None)
            .unwrap()
            .download(r.clone(), CancellationToken::new(), Progress::default())
            .await
            .is_err()
    );
    assert!(!r.destination.exists());
}

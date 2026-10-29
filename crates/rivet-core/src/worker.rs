use crate::{CancellationToken, DownloadResult, Engine, Progress, queue::Queue};
use anyhow::{Result, ensure};
use std::time::Duration;

/// Runs one claimed job and persists its terminal state. No job is auto-started here.
pub async fn run_job(
    queue: Queue,
    id: &str,
    engine: Engine,
    cancel: CancellationToken,
    progress: Progress,
) -> Result<DownloadResult> {
    let owner = uuid::Uuid::new_v4().to_string();
    ensure!(
        queue.claim(id, &owner)?,
        "job is already active, completed, or scheduled for later"
    );
    let request = queue.get(id)?.request;
    let transfer_cancel = cancel.child_token();
    let future = engine.download(request, transfer_cancel.clone(), progress.clone());
    tokio::pin!(future);
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    let result = loop {
        tokio::select! {
            result = &mut future => break result,
            _ = interval.tick() => {
                let (bytes, total) = progress.snapshot();
                if let Err(error) = queue.heartbeat(id, &owner, bytes, total) {
                    transfer_cancel.cancel();
                    let _ = (&mut future).await;
                    return Err(error);
                }
            }
        }
    };
    match &result {
        Ok(done) => queue.finish(id, &owner, "complete", done.bytes, Some(&done.sha256), None)?,
        Err(error) => queue.finish(
            id,
            &owner,
            if cancel.is_cancelled() {
                "paused"
            } else {
                "failed"
            },
            progress.snapshot().0,
            None,
            Some(&safe_error(error)),
        )?,
    }
    result
}

pub fn safe_error(error: &anyhow::Error) -> String {
    // reqwest's Display can include signed URL query parameters.
    if error.chain().any(|e| e.is::<reqwest::Error>()) {
        "Network request failed; check connectivity, TLS, proxy settings, or server response."
            .into()
    } else {
        error.to_string()
    }
}

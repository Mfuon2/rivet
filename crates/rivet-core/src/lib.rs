pub mod browser_install;
pub mod browser_setup;
pub mod download;
pub mod native;
pub mod progress;
pub mod queue;
pub mod update;
pub mod worker;

pub use download::{DownloadRequest, DownloadResult, Engine, Progress};
pub use progress::{SegmentSnapshot, TransferSnapshot, TransferState};
pub use tokio_util::sync::CancellationToken;

pub mod desktop;
pub mod media;

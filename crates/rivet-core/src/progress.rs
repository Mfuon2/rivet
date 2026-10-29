//! Live transfer telemetry shared by the engine and desktop interface.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TransferState {
    #[default]
    Waiting,
    Connecting,
    Receiving,
    Retrying,
    Assembling,
    Verifying,
    Complete,
    Paused,
    Failed,
}
impl TransferState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Waiting => "Waiting",
            Self::Connecting => "Connecting",
            Self::Receiving => "Receiving data",
            Self::Retrying => "Retrying",
            Self::Assembling => "Assembling segments",
            Self::Verifying => "Verifying SHA-256",
            Self::Complete => "Complete",
            Self::Paused => "Paused",
            Self::Failed => "Failed",
        }
    }
    fn load(value: &AtomicU8) -> Self {
        match value.load(Ordering::Relaxed) {
            1 => Self::Connecting,
            2 => Self::Receiving,
            3 => Self::Retrying,
            4 => Self::Assembling,
            5 => Self::Verifying,
            6 => Self::Complete,
            7 => Self::Paused,
            8 => Self::Failed,
            _ => Self::Waiting,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SegmentSnapshot {
    pub id: usize,
    pub start: u64,
    pub end: Option<u64>,
    pub total: Option<u64>,
    pub downloaded: u64,
    pub state: TransferState,
}
#[derive(Clone, Debug)]
pub struct TransferSnapshot {
    pub state: TransferState,
    pub resumable: bool,
    pub segments: Vec<SegmentSnapshot>,
}

pub(crate) struct SegmentProgress {
    start: u64,
    total: Option<u64>,
    pub(crate) downloaded: AtomicU64,
    state: AtomicU8,
}
impl SegmentProgress {
    pub(crate) fn new(start: u64, total: Option<u64>) -> Arc<Self> {
        Arc::new(Self {
            start,
            total,
            downloaded: AtomicU64::new(0),
            state: AtomicU8::new(TransferState::Waiting as u8),
        })
    }
    pub(crate) fn set_state(&self, state: TransferState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }
}

#[derive(Clone, Default)]
pub struct Progress {
    pub completed: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    state: Arc<AtomicU8>,
    resumable: Arc<AtomicBool>,
    segments: Arc<Mutex<Vec<Arc<SegmentProgress>>>>,
}
impl Progress {
    pub fn snapshot(&self) -> (u64, u64) {
        (
            self.completed.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }
    pub fn transfer_snapshot(&self) -> TransferSnapshot {
        let segments = self.segments.lock().unwrap_or_else(|e| e.into_inner());
        TransferSnapshot {
            state: TransferState::load(&self.state),
            resumable: self.resumable.load(Ordering::Relaxed),
            segments: segments
                .iter()
                .enumerate()
                .map(|(id, segment)| SegmentSnapshot {
                    id: id + 1,
                    start: segment.start,
                    end: segment
                        .total
                        .and_then(|n| n.checked_sub(1))
                        .map(|n| segment.start + n),
                    total: segment.total,
                    downloaded: segment.downloaded.load(Ordering::Relaxed),
                    state: TransferState::load(&segment.state),
                })
                .collect(),
        }
    }
    pub(crate) fn set_state(&self, state: TransferState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }
    pub(crate) fn initialize(&self, segments: Vec<Arc<SegmentProgress>>, resumable: bool) {
        *self.segments.lock().unwrap_or_else(|e| e.into_inner()) = segments;
        self.resumable.store(resumable, Ordering::Relaxed);
    }
    pub(crate) fn finish(&self, state: TransferState) {
        let segments = self.segments.lock().unwrap_or_else(|e| e.into_inner());
        for segment in segments.iter() {
            if TransferState::load(&segment.state) != TransferState::Complete {
                segment.set_state(state);
            }
        }
        self.set_state(state);
    }
}

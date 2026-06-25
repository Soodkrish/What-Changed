/// Shared event types emitted from Rust backend to the frontend.
/// Used by both lib.rs (manual scan) and scheduler.rs (periodic scan).

#[derive(Clone, serde::Serialize)]
pub struct ScanProgressEvent {
    pub current: usize,
    pub total: usize,
    pub directory: String,
    pub phase: String,
    pub progress_percent: u32,
    pub files_scanned: i64,
}

//! Process cancellation boundary shared by application and media children.
use std::sync::atomic::{AtomicBool, Ordering};
static CANCELLED: AtomicBool = AtomicBool::new(false);
pub fn request() {
    CANCELLED.store(true, Ordering::Release);
}
pub fn is_cancelled() -> bool {
    CANCELLED.load(Ordering::Acquire)
}
pub fn check() -> Result<(), String> {
    if is_cancelled() {
        Err("Processing cancelled; originals were not archived.".into())
    } else {
        Ok(())
    }
}
/// Called by the CLI once, not by library consumers or individual workers.
pub fn install_handler() -> Result<(), String> {
    ctrlc::set_handler(request).map_err(|e| format!("Could not install cancellation handler: {e}"))
}

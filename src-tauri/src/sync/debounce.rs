use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::sleep;

const DEBOUNCE_DELAY: Duration = Duration::from_secs(5);

/// Owned by the scheduler. `wait_for_fire` returns once the debounce timer
/// expires (5s of inactivity) and sync is not inhibited.
pub struct DebounceTrigger {
    pub rx: watch::Receiver<u64>,
    inhibited: Arc<AtomicBool>,
    generation: Arc<std::sync::atomic::AtomicU64>,
}

impl DebounceTrigger {
    pub async fn wait_for_fire(&mut self) -> u64 {
        loop {
            self.rx.changed().await.ok();
            let fired_gen = *self.rx.borrow();

            sleep(DEBOUNCE_DELAY).await;

            let current_gen = self.generation.load(Ordering::Relaxed);
            if current_gen == fired_gen && !self.inhibited.load(Ordering::Relaxed) {
                return fired_gen;
            }
        }
    }
}

/// Create a debounce trigger. Call `notify_mutation` on the sender after
/// DB writes to reset the timer; the trigger is consumed by the scheduler.
pub fn new_debounce() -> DebounceTrigger {
    let generation = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let inhibited = Arc::new(AtomicBool::new(false));
    let (_tx, rx) = watch::channel(0u64);

    DebounceTrigger {
        rx,
        inhibited,
        generation,
    }
}

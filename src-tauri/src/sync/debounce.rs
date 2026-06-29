use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::sleep;

const DEBOUNCE_DELAY: Duration = Duration::from_secs(5);

/// Consumer half — owned by the scheduler. `wait_for_fire` blocks until
/// `notify_mutation` has been called and the trailing quiet window
/// (`DEBOUNCE_DELAY`) has elapsed.
pub struct DebounceTrigger {
    rx: watch::Receiver<u64>,
    inhibited: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
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

/// Producer half — held by write commands (via Tauri state). Calling
/// `notify_mutation` resets the scheduler's debounce window so the next
/// `wait_for_fire` resolves `DEBOUNCE_DELAY` after the *most recent*
/// mutation rather than after an earlier one.
#[derive(Clone)]
pub struct DebounceSender {
    tx: Arc<watch::Sender<u64>>,
    generation: Arc<AtomicU64>,
}

impl DebounceSender {
    pub fn notify_mutation(&self) {
        let next = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.tx.send(next);
    }
}

/// Create the matched (trigger, sender) pair for one app lifetime.
pub fn new_debounce() -> (DebounceTrigger, DebounceSender) {
    let generation = Arc::new(AtomicU64::new(0));
    let inhibited = Arc::new(AtomicBool::new(false));
    let (tx, rx) = watch::channel(0u64);

    let trigger = DebounceTrigger {
        rx,
        inhibited,
        generation: generation.clone(),
    };
    let sender = DebounceSender {
        tx: Arc::new(tx),
        generation,
    };
    (trigger, sender)
}

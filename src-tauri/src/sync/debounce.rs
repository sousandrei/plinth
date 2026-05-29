use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::sleep;

const DEBOUNCE_DELAY: Duration = Duration::from_secs(5);

/// Cloneable handle vended to the rest of the app. Commands call
/// `notify_mutation` after any DB write; bulk import flows call
/// `set_inhibited(true)` / `set_inhibited(false)`.
#[derive(Clone)]
pub struct DebounceHandle {
    trigger_tx: watch::Sender<u64>,
    inhibited: Arc<AtomicBool>,
    generation: Arc<std::sync::atomic::AtomicU64>,
}

impl DebounceHandle {
    /// Signal that a DB mutation just happened. Resets the 5s sliding timer.
    pub fn notify_mutation(&self) {
        let gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.trigger_tx.send(gen);
    }

    /// Suspend sync for the duration of a bulk import (`true`) or release
    /// it (`false`). Calling `notify_mutation` while inhibited is a no-op
    /// for the timer — the debounce task skips firing until inhibition clears.
    /// When inhibition is released the caller should call `notify_mutation`
    /// once to schedule a single unified sync pass.
    pub fn set_inhibited(&self, inhibited: bool) {
        self.inhibited.store(inhibited, Ordering::Relaxed);
    }

    pub fn is_inhibited(&self) -> bool {
        self.inhibited.load(Ordering::Relaxed)
    }
}

/// Receiver side — handed to the scheduler so it can `await` debounce fires.
pub struct DebounceTrigger {
    pub rx: watch::Receiver<u64>,
    inhibited: Arc<AtomicBool>,
    generation: Arc<std::sync::atomic::AtomicU64>,
}

impl DebounceTrigger {
    /// Wait until the debounce timer fires (5s since the last mutation with
    /// no new mutations arriving) and sync is not inhibited. Returns the
    /// generation counter of the firing event.
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

/// Create a linked `(DebounceHandle, DebounceTrigger)` pair. The handle is
/// cloned freely across the app; the trigger is owned by the scheduler.
pub fn new_debounce() -> (DebounceHandle, DebounceTrigger) {
    let generation = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let inhibited = Arc::new(AtomicBool::new(false));
    let (tx, rx) = watch::channel(0u64);

    let handle = DebounceHandle {
        trigger_tx: tx,
        inhibited: inhibited.clone(),
        generation: generation.clone(),
    };
    let trigger = DebounceTrigger {
        rx,
        inhibited,
        generation,
    };
    (handle, trigger)
}

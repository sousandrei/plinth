pub mod apply;
pub mod apply_guard;
pub mod cert_match;
pub mod cert_validation;
pub mod changelog;
pub mod client;
pub mod conflict_detector;
pub mod cursors;
pub mod debounce;
pub mod discovery;
pub mod frame;
pub mod gc;
pub mod identity;
pub mod model_sync;
pub mod pairing;
pub mod payloads;
pub mod scheduler;
pub mod server;
pub mod session;
pub mod snapshot;
pub mod startup;
pub mod tls;
pub mod trust_mode;
pub mod wire;

#[cfg(test)]
pub mod harness;

pub use discovery::{PeerInfo, PeerRegistry};
pub use pairing::PairingState;
pub use scheduler::SyncSummary;

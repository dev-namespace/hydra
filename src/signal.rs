use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

/// Signal state constants
const SIGNAL_NONE: u8 = 0;
const SIGNAL_TERM: u8 = 1; // SIGTERM - graceful shutdown
const SIGNAL_INT: u8 = 2; // SIGINT - immediate exit

/// Global signal state
static SIGNAL_RECEIVED: AtomicU8 = AtomicU8::new(SIGNAL_NONE);

/// Check if an immediate exit signal was received
pub fn is_immediate_exit() -> bool {
    SIGNAL_RECEIVED.load(Ordering::SeqCst) == SIGNAL_INT
}

/// Check if a graceful shutdown signal was received
pub fn is_graceful_shutdown() -> bool {
    SIGNAL_RECEIVED.load(Ordering::SeqCst) == SIGNAL_TERM
}

/// Check if any signal was received
pub fn any_signal_received() -> bool {
    SIGNAL_RECEIVED.load(Ordering::SeqCst) != SIGNAL_NONE
}

/// Install signal handlers
///
/// - SIGINT (Ctrl+C): Sets immediate exit flag
/// - SIGTERM: Sets graceful shutdown flag
///
/// The `stop_flag` is set to true on any signal, allowing the runner
/// to check a single flag for stopping.
pub fn install_handlers(stop_flag: Arc<AtomicBool>) -> Result<(), ctrlc::Error> {
    ctrlc::set_handler(move || {
        // Check what signal we received
        // ctrlc with termination feature handles both SIGINT and SIGTERM
        // On first signal, request graceful shutdown
        // On second signal (or if already set), force immediate exit
        let current = SIGNAL_RECEIVED.load(Ordering::SeqCst);

        if current == SIGNAL_NONE {
            // First signal - treat as SIGTERM (graceful)
            // Note: ctrlc doesn't distinguish between SIGINT/SIGTERM,
            // so we use double-tap detection for immediate exit
            SIGNAL_RECEIVED.store(SIGNAL_TERM, Ordering::SeqCst);
            stop_flag.store(true, Ordering::SeqCst);
            eprintln!("\n[hydra] Received interrupt, finishing current iteration... (press Ctrl+C again to force quit)");
        } else {
            // Second signal - immediate exit
            SIGNAL_RECEIVED.store(SIGNAL_INT, Ordering::SeqCst);
            eprintln!("\n[hydra] Force quit!");
            std::process::exit(1);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_state_constants() {
        assert_ne!(SIGNAL_NONE, SIGNAL_TERM);
        assert_ne!(SIGNAL_NONE, SIGNAL_INT);
        assert_ne!(SIGNAL_TERM, SIGNAL_INT);
    }

    #[test]
    fn test_initial_state() {
        // Note: This test may be affected by other tests that modify global state
        // In a fresh process, the initial state should be SIGNAL_NONE
        // We can't reliably test this without process isolation
    }
}

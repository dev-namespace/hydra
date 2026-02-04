use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};

/// Signal state constants
const SIGNAL_NONE: u8 = 0;
const SIGNAL_TERM: u8 = 1; // SIGTERM - graceful shutdown
const SIGNAL_INT: u8 = 2; // SIGINT - immediate exit

/// Global signal state
static SIGNAL_RECEIVED: AtomicU8 = AtomicU8::new(SIGNAL_NONE);

/// Global child process PID for signal handler to kill
/// 0 means no child process
static CHILD_PID: AtomicI32 = AtomicI32::new(0);

/// Set the child process PID for signal handling
pub fn set_child_pid(pid: u32) {
    CHILD_PID.store(pid as i32, Ordering::SeqCst);
}

/// Clear the child process PID
pub fn clear_child_pid() {
    CHILD_PID.store(0, Ordering::SeqCst);
}

/// Kill the child process group with SIGTERM
fn kill_child_process_group() {
    let pid = CHILD_PID.load(Ordering::SeqCst);
    if pid > 0 {
        // Kill the process group (the claude process)
        let _ = killpg(Pid::from_raw(pid), Signal::SIGTERM);
    }
}

/// Kill the child process group with SIGKILL (forceful)
fn force_kill_child_process_group() {
    let pid = CHILD_PID.load(Ordering::SeqCst);
    if pid > 0 {
        let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
    }
}

/// Handle interrupt (called from signal handler)
fn handle_interrupt(stop_flag: &Arc<AtomicBool>) {
    let current = SIGNAL_RECEIVED.load(Ordering::SeqCst);

    if current == SIGNAL_NONE {
        // First signal - graceful shutdown
        SIGNAL_RECEIVED.store(SIGNAL_TERM, Ordering::SeqCst);
        stop_flag.store(true, Ordering::SeqCst);
        // Kill the child process group immediately so Claude stops
        kill_child_process_group();
        eprintln!(
            "\n[hydra] Received interrupt, finishing current iteration... (press Ctrl+C again to force quit)"
        );
    } else {
        // Second signal - immediate exit
        SIGNAL_RECEIVED.store(SIGNAL_INT, Ordering::SeqCst);
        force_kill_child_process_group();
        eprintln!("\n[hydra] Force quit!");
        std::process::exit(1);
    }
}

/// Install signal handlers
///
/// - SIGINT (Ctrl+C): Sets immediate exit flag
/// - SIGTERM: Sets graceful shutdown flag
///
/// The `stop_flag` is set to true on any signal, allowing the runner
/// to check a single flag for stopping.
pub fn install_handlers(stop_flag: Arc<AtomicBool>) -> Result<(), ctrlc::Error> {
    let stop_flag_clone = stop_flag.clone();
    ctrlc::set_handler(move || {
        handle_interrupt(&stop_flag_clone);
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
}

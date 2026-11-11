//! Stopping cleanly when the user presses Ctrl-C.

use std::sync::atomic::{AtomicBool, Ordering};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Die quietly when the output is piped into something that stops reading,
/// the way any other command does.
pub fn default_pipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// Catch Ctrl-C so the tool can stop tidily instead of being killed.
pub fn install() {
    unsafe {
        let handler = on_interrupt as *const () as libc::sighandler_t;
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
}

/// Keep going when the terminal that started the tool goes away.
pub fn ignore_hangup() {
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
}

/// Whether the user asked the tool to stop.
pub fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

extern "C" fn on_interrupt(_signal: libc::c_int) {
    REQUESTED.store(true, Ordering::SeqCst);
}

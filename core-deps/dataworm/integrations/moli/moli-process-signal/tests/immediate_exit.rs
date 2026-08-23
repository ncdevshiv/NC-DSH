#![cfg(unix)]

use std::{io, sync::mpsc, thread, time::Duration};

#[test]
fn termination_signals_exit_immediately_with_conventional_status() {
    moli_process_signal::install_immediate_exit_handlers()
        .expect("termination signal handlers should install");

    for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        assert_signal_exit(signal);
    }
}

fn assert_signal_exit(signal: libc::c_int) {
    // SAFETY: The child performs no Rust or libc work after fork except the
    // async-signal-safe pause call. It inherits the handlers installed above
    // and exits through their async-signal-safe `_exit` call.
    let child_pid = unsafe { libc::fork() };
    assert_ne!(child_pid, -1, "fork failed: {}", io::Error::last_os_error());

    if child_pid == 0 {
        loop {
            // SAFETY: pause has no pointer or ownership preconditions. A
            // handled termination signal ends this child instead of returning.
            unsafe {
                libc::pause();
            }
        }
    }

    // SAFETY: child_pid names the live fork child and signal is one of the
    // three valid termination signals above.
    assert_eq!(unsafe { libc::kill(child_pid, signal) }, 0);

    let (status_sender, status_receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = status_sender.send(wait_for_child(child_pid));
    });

    let wait_status = match status_receiver.recv_timeout(Duration::from_secs(2)) {
        Ok(status) => status.expect("waiting for signal target should succeed"),
        Err(error) => {
            // SAFETY: child_pid still identifies the timed-out child. SIGKILL
            // is used only to keep a failed regression test from leaking it.
            unsafe {
                libc::kill(child_pid, libc::SIGKILL);
            }
            panic!("signal target did not exit promptly: {error}");
        }
    };

    assert!(libc::WIFEXITED(wait_status));
    assert_eq!(libc::WEXITSTATUS(wait_status), 128 + signal);
}

fn wait_for_child(child_pid: libc::pid_t) -> io::Result<libc::c_int> {
    let mut status = 0;
    loop {
        // SAFETY: status is writable for the duration of the call and this
        // thread is the only waiter for child_pid.
        let result = unsafe { libc::waitpid(child_pid, &mut status, 0) };
        if result == child_pid {
            return Ok(status);
        }
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

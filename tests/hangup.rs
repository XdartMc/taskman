//! Closing taskman's terminal ends taskman and its sessions. Idle, the loop sees it (`Tty::wait`)
//! and quits as on `prefix q`. Part-way through input, crossterm 0.29 reads on until the sequence
//! ends, retrying the end of file forever, so this is the watchdog's job (see `watchdog` in
//! src/main.rs).

mod common;

use common::{answer_queries, scratch_dir, spawn_in_pty, stub_cli, taskman};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
fn hang_up_while_idle() {
    // Quit as on `prefix q`: not ended by the watchdog, nor aborted by a write to the terminal.
    let status = hang_up_during(b"");
    assert!(status.success(), "{status}");
}

#[test]
fn hang_up_during_a_paste() {
    hang_up_during(b"\x1b[200~half of a paste");
}

#[test]
fn hang_up_during_a_mouse_report() {
    hang_up_during(b"\x1b[<35;10;");
}

#[test]
fn hang_up_during_a_key() {
    hang_up_during(b"\x1b[1;");
}

fn hang_up_during(input: &[u8]) -> std::process::ExitStatus {
    let dir = scratch_dir("hangup");
    let pid_file = dir.join("session.pid");
    let (mut taskman, mut master) = start(&dir, &pid_file);
    // The heartbeat `taskman add` looks for.
    let alive = dir.join("state/taskman/inbox/alive");

    // Starting: answer crossterm's keyboard query, until the ticket's session is running.
    let until = Instant::now() + Duration::from_secs(10);
    let session = loop {
        if let Some(pid) =
            std::fs::read_to_string(&pid_file).ok().and_then(|s| s.trim().parse::<i32>().ok())
            && alive.exists()
        {
            break pid;
        }
        assert!(Instant::now() < until, "the session didn't start");
        let mut buf = [0u8; 65536];
        if let Ok(n) = master.read(&mut buf) {
            answer_queries(&mut master, &buf[..n]);
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    master.write_all(input).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    drop(master);

    let until = Instant::now() + Duration::from_secs(15);
    let ended = loop {
        if let Some(status) = taskman.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() > until {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if ended.is_none() {
        let _ = taskman.kill();
        let _ = taskman.wait();
    }
    // Its parent gone, the session is reaped by init once it ends.
    let until = Instant::now() + Duration::from_secs(5);
    // SAFETY: signal 0 only checks the process exists.
    while unsafe { libc::kill(session, 0) } == 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    // SAFETY: as above.
    let session_left = unsafe { libc::kill(session, 0) } == 0;
    if session_left {
        // SAFETY: sends a signal; no memory is involved.
        unsafe { libc::kill(session, libc::SIGKILL) };
    }
    let alive_left = alive.exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ended.is_some(), "taskman was still running 15s after its terminal closed");
    assert!(!session_left, "the session was still running after taskman ended");
    assert!(!alive_left, "taskman ended without removing its heartbeat");
    ended.unwrap()
}

/// taskman in a new pty, as its controlling terminal, with one ticket whose session is a stub CLI
/// that writes its pid to `pid_file` and sleeps. Returns taskman and the pty's master side.
fn start(dir: &Path, pid_file: &Path) -> (std::process::Child, std::fs::File) {
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("workplan/todo")).unwrap();
    std::fs::write(repo.join("workplan/todo/ENG-1-test.md"), "# test\n").unwrap();
    let state = dir.join("state/taskman");
    std::fs::create_dir_all(&state).unwrap();
    let tickets = serde_json::json!({
        "projects": [{"name": "acme", "repo": repo, "tracker": {"provider": "local", "site": "workplan", "key": "ENG"}}],
        "tickets": [{"key": "ENG-1", "repo": repo, "cwd": repo}],
    });
    std::fs::write(state.join("tickets.json"), tickets.to_string()).unwrap();
    let mut cmd = taskman(dir);
    cmd.env("TASKMAN_CLAUDE", stub_cli(dir, pid_file));
    spawn_in_pty(cmd)
}

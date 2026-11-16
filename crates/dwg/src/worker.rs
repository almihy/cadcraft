//! The DWG reader in a child process.
//!
//! An allocation failure or stack overflow in the third-party decoder ends the process it runs
//! in, and `catch_unwind` can't stop that. So a native app reads each DWG first in a copy of
//! itself: [`use_own_worker`] says how to start one, and [`serve_if_asked`], first thing in
//! `main`, turns the copy into a worker that reads one request from standard input and writes one
//! reply to standard output. The worker decodes the file and translates it as the app would; when
//! it comes back unharmed, the app reads the same bytes itself (acadrust's decoding is
//! deterministic), and when it doesn't, the file costs that one open and every other open drawing
//! stays safe.
//!
//! Without a worker (libraries, tests, the browser, a build that can't start one) the file is
//! read in the calling process alone. There's no memory limit on the worker yet: that takes
//! per-platform system calls, which `unsafe_code = "forbid"` keeps out of this crate.
//!
//! A reply comes from a process that just handled a possibly hostile file, so it is parsed as
//! untrusted input.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use crate::Limits;

/// The argument that makes the app a worker.
pub const WORKER_ARG: &str = "--dwg-worker";
/// Bumped when the messages change: an app updated while running may start a newer copy.
const PROTOCOL: u8 = 3;
const READ: u8 = 1;
const CRASH: u8 = 0xff;
/// How long one file may take before the worker is stopped. It can't be interrupted otherwise.
const TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Most bytes accepted as a reply (the notes on what the read left out).
const MAX_REPLY: u64 = 64 << 20;
/// Most bytes one request holds: the largest input and its header.
const MAX_REQUEST: u64 = (2 << 30) + 256;

struct Worker {
    program: PathBuf,
    args: Vec<OsString>,
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

/// Start `program args` as the worker from now on. `program` must call [`serve_if_asked`].
pub fn use_worker(program: PathBuf, args: Vec<OsString>) {
    if let Ok(mut w) = WORKER.lock() {
        *w = Some(Worker { program, args });
    }
}

/// Use a copy of this program as the worker, started with [`WORKER_ARG`]. If its path can't be
/// found, files are read in this process only.
pub fn use_own_worker() {
    match std::env::current_exe() {
        Ok(program) => use_worker(program, vec![WORKER_ARG.into()]),
        Err(e) => log::warn!("DWG worker: can't find this program: {e}"),
    }
}

/// Serve one request and return the exit code if this process was started as a worker.
pub fn serve_if_asked() -> Option<i32> {
    (std::env::args_os().nth(1)? == WORKER_ARG).then(|| serve(std::io::stdin().lock(), std::io::stdout().lock()))
}

/// Read `bytes` in the worker, when there is one: `Ok` when it read them unharmed (or there is no
/// worker), the reason when it couldn't.
pub fn probe(bytes: &[u8], limits: Limits) -> Result<(), String> {
    match call(&request(READ, limits), bytes, "reading this file") {
        Some(reply) => reply.map(|_| ()),
        None => Ok(()),
    }
}

/// Make the worker abort, as the third-party code can: what the tests prove isolation with.
#[doc(hidden)]
pub fn crash_worker_for_test() -> Result<(), String> {
    call(&request(CRASH, Limits::DEFAULT), &[], "testing").ok_or("no worker")?.map(|_| ())
}

fn request(op: u8, limits: Limits) -> Vec<u8> {
    let mut r = vec![PROTOCOL, op];
    r.extend((limits.max_dwg_bytes as u64).to_le_bytes());
    r.extend((limits.max_records as u64).to_le_bytes());
    r
}

/// Run one request in a worker. `None`: none could be started, so the caller goes on alone.
/// `Some(Err)`: the file couldn't be read, or the worker went wrong; `doing` says at what, for
/// the user.
fn call(header: &[u8], payload: &[u8], doing: &str) -> Option<Result<Vec<String>, String>> {
    let mut command = {
        let worker = WORKER.lock().ok()?;
        let worker = worker.as_ref()?;
        let mut c = Command::new(&worker.program);
        c.args(&worker.args);
        c
    };
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("DWG worker: can't start it: {e}");
            return None;
        }
    };
    let (Some(mut to_child), Some(from_child)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let received = std::thread::scope(|scope| {
        // Written beside the read: a worker that replies before it has read everything, or dies,
        // must block neither side. A failed write shows up as the reply going wrong.
        scope.spawn(move || {
            let _ = to_child.write_all(header).and_then(|()| to_child.write_all(payload));
        });
        let (tx, rx) = channel();
        scope.spawn(move || {
            let mut reply = Vec::new();
            let read = from_child.take(MAX_REPLY).read_to_end(&mut reply);
            let _ = tx.send(read.map(|_| reply));
        });
        let received = rx.recv_timeout(TIMEOUT);
        if matches!(received, Err(RecvTimeoutError::Timeout)) {
            let _ = child.kill();
        }
        received
    });
    let status = child.wait();
    let stopped = || format!("the DWG reader stopped while {doing}; nothing else is affected");
    Some(match (received, status) {
        (Err(RecvTimeoutError::Timeout), _) => Err(format!("{doing} took too long, so the DWG reader was stopped; nothing else is affected")),
        (Ok(Ok(reply)), Ok(status)) if status.success() => parse_reply(&reply).ok_or_else(stopped).and_then(|r| r),
        _ => Err(stopped()),
    })
}

/// Take `n` bytes off the front of `b`.
fn take<'a>(b: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    let (head, tail) = b.split_at_checked(n)?;
    *b = tail;
    Some(head)
}

fn take_u32(b: &mut &[u8]) -> Option<usize> {
    let bytes: [u8; 4] = take(b, 4)?.try_into().ok()?;
    usize::try_from(u32::from_le_bytes(bytes)).ok()
}

fn take_u64(b: &mut &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(take(b, 8)?.try_into().ok()?))
}

type Reply = Result<Vec<String>, String>;

/// A reply: 0, the note count and each note with its length; or 1 and a message. `None` when it
/// isn't one, whole.
fn parse_reply(reply: &[u8]) -> Option<Reply> {
    let mut b = reply;
    match *take(&mut b, 1)?.first()? {
        0 => {
            let count = take_u32(&mut b)?;
            let mut notes = Vec::new();
            for _ in 0..count {
                let n = take_u32(&mut b)?;
                notes.push(String::from_utf8_lossy(take(&mut b, n)?).into_owned());
            }
            b.is_empty().then_some(Ok(notes))
        }
        1 => Some(Err(String::from_utf8_lossy(b).into_owned())),
        _ => None,
    }
}

fn write_reply(out: &mut impl Write, reply: &Reply) -> std::io::Result<()> {
    match reply {
        Ok(notes) => {
            out.write_all(&[0])?;
            let notes: Vec<&String> = notes.iter().filter(|n| u32::try_from(n.len()).is_ok()).collect();
            out.write_all(&u32::try_from(notes.len()).unwrap_or(0).to_le_bytes())?;
            for note in notes {
                out.write_all(&u32::try_from(note.len()).unwrap_or(0).to_le_bytes())?;
                out.write_all(note.as_bytes())?;
            }
        }
        Err(e) => {
            out.write_all(&[1])?;
            out.write_all(e.as_bytes())?;
        }
    }
    out.flush()
}

/// What a request asks for, or why it can't be served.
fn handle(mut b: &[u8]) -> Reply {
    let bad = || "the DWG reader got a request it can't read (a different version of the app?)".to_string();
    if take(&mut b, 1).and_then(<[u8]>::first) != Some(&PROTOCOL) {
        return Err(bad());
    }
    let op = take(&mut b, 1).and_then(<[u8]>::first).copied().ok_or_else(bad)?;
    let max_dwg_bytes = take_u64(&mut b).and_then(|n| usize::try_from(n).ok()).ok_or_else(bad)?;
    let max_records = take_u64(&mut b).and_then(|n| usize::try_from(n).ok()).ok_or_else(bad)?;
    match op {
        // The whole read, as the app will do it: decoding and translation.
        READ => crate::read_here(b, Limits { max_dwg_bytes, max_records }).map(|(_, notes)| notes),
        CRASH => std::process::abort(),
        _ => Err(bad()),
    }
}

/// Serve the one request on `input`. The exit code is 0 unless the reply couldn't be sent.
fn serve(input: impl Read, mut output: impl Write) -> i32 {
    let mut request = Vec::new();
    let reply = match input.take(MAX_REQUEST).read_to_end(&mut request) {
        Ok(_) => handle(&request),
        Err(e) => Err(format!("the DWG reader could not read its request: {e}")),
    };
    i32::from(write_reply(&mut output, &reply).is_err())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_round_trip_and_damaged_ones_are_refused() {
        let mut out = Vec::new();
        write_reply(&mut out, &Ok(vec!["a note".into(), String::new()])).unwrap();
        assert_eq!(parse_reply(&out), Some(Ok(vec!["a note".to_string(), String::new()])));
        out.push(0);
        assert_eq!(parse_reply(&out), None);
        let mut err = Vec::new();
        write_reply(&mut err, &Err("no".into())).unwrap();
        assert_eq!(parse_reply(&err), Some(Err("no".into())));
        assert_eq!(parse_reply(&[]), None);
        assert_eq!(parse_reply(&[0, 5, 0, 0, 0]), None);
        assert_eq!(parse_reply(&[7]), None);
    }

    #[test]
    fn a_request_of_another_protocol_or_a_file_that_is_not_a_dwg_is_refused() {
        let mut out = Vec::new();
        serve(&[PROTOCOL + 1, READ][..], &mut out);
        assert!(matches!(parse_reply(&out), Some(Err(_))));
        let mut out = Vec::new();
        let mut req = request(READ, Limits::DEFAULT);
        req.extend(b"AC1032 not really a drawing");
        serve(&req[..], &mut out);
        assert!(matches!(parse_reply(&out), Some(Err(e)) if e.starts_with("DWG")), "{:?}", parse_reply(&out));
    }
}

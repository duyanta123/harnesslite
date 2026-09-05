//! A real terminal, inside the window.
//!
//! Not a log view and not a command runner: a pty with a shell in it, so that
//! `dsh` — and git, and npm, and whatever else the work needs — can be run
//! without leaving the application. A pseudo-terminal rather than piped stdio,
//! because line editing, colour, Ctrl-C and progress bars are all things a
//! program does only when it believes it is talking to a terminal.
//!
//! Two decisions are worth stating outright.
//!
//! **The pty runs under its own [`ProcessGuard`], not the supervisor's.** Sharing
//! one would tie the two lifetimes together in the wrong direction: stopping the
//! harness would take the user's shells down with it, half-typed commands and
//! all. What matters is the guarantee, and a second guard gives exactly the same
//! one — on Windows a Job Object with `KILL_ON_JOB_CLOSE`, so closing the window
//! is enough to reclaim every process a terminal started, whether or not any
//! code of ours gets to run.
//!
//! **Each session owns three threads, and none of them is async.** `portable-pty`
//! is a blocking API, and wrapping blocking reads in tasks would either occupy a
//! runtime worker for the life of the terminal or lose the ordering that a
//! terminal is made of. Threads are the honest shape: one reading, one writing,
//! one waiting for the child to exit.

pub mod decoder;
pub mod shell;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use proc_guard::ProcessGuard;
use serde::Serialize;

use hd_core::error::{Error, Result};

use super::terminal::decoder::Decoder;

/// Read buffer size. A pty read returns whatever has arrived, so this is a
/// ceiling rather than a batch size — large enough that a burst of build output
/// arrives in a few events instead of hundreds.
const READ_CHUNK: usize = 32 * 1024;

/// A terminal the frontend can address.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Handle for every other command. Unique for the life of the application.
    pub id: String,
    /// What is running, for the tab: `pwsh`, `bash`, `fish`.
    pub label: String,
    /// Directory the shell started in, which is what the tab's tooltip shows.
    pub cwd: PathBuf,
}

/// An event a terminal produces, decoded and ready for the frontend.
#[derive(Clone, Debug)]
pub enum Event {
    Output { id: String, data: String },
    Exit { id: String, code: Option<u32> },
}

/// Where terminal events go. The shell layer binds this to its event relay.
pub type Emit = Arc<dyn Fn(Event) + Send + Sync>;

/// What opening a terminal needs, resolved by the caller.
pub struct OpenSpec {
    pub rows: u16,
    pub cols: u16,
    /// Where the shell starts: the active project's folder.
    pub cwd: PathBuf,
    /// Written into the pty before the shell can draw its prompt.
    pub banner: String,
    /// `PATH` the child resolves its shell and its tools on.
    pub path: OsString,
    /// Extra environment (identity markers, profile, home).
    pub environment: HashMap<String, String>,
}

/// What a live session needs to be steered.
struct Live {
    /// Kept for `resize` — and because dropping it is what closes the pty.
    master: Box<dyn MasterPty + Send>,
    /// Bytes queued for the shell. Dropping it stops the writer thread.
    input: mpsc::Sender<Vec<u8>>,
    /// Ends the shell without waiting for it.
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// Repeated back by `list`, so a reopened pane can rebuild its tabs.
    describe: Session,
}

/// Every terminal this window has open.
pub struct Terminals {
    /// Own guard, deliberately. See the module comment.
    guard: ProcessGuard,
    live: Mutex<HashMap<String, Live>>,
    /// Session ids are handed out from here, never reused within a run.
    next: AtomicU64,
}

impl Terminals {
    pub fn new() -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            guard: ProcessGuard::new().map_err(|failure| {
                Error::Terminal(format!("process reclamation is unavailable: {failure}"))
            })?,
            live: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        }))
    }

    /// Open a terminal and start reading from it.
    ///
    /// `rows` and `cols` are the size the pane already knows it has. Starting at
    /// the right size matters more than it sounds: a shell asks once, at startup,
    /// and a prompt drawn for 80 columns stays wrong until something redraws it.
    /// The `Arc<Self>` return on [`Terminals::new`] is what lets the waiter
    /// thread keep the whole registry — and with it the process guard — alive
    /// until the shell it waits on is done.
    pub fn open(self: &Arc<Self>, spec: OpenSpec, emit: Emit) -> Result<Session> {
        let pty = portable_pty::native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: spec.rows.max(1),
                cols: spec.cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|failure| Error::Terminal(format!("could not open a terminal: {failure}")))?;

        let mut command = CommandBuilder::from_argv(shell::argv(&spec.path));
        for (name, value) in &spec.environment {
            command.env(name, value);
        }
        command.env("PATH", &spec.path);
        command.cwd(&spec.cwd);

        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|failure| Error::Terminal(format!("could not start the shell: {failure}")))?;

        // Before anything else is set up: from here on the shell is the kernel's
        // problem to reclaim, however this process ends. A pid is all the pty
        // layer offers, and on Windows that leaves a gap of microseconds between
        // creation and adoption — `ProcessGuard::adopt` says so in full.
        if let Some(pid) = child.process_id() {
            if let Err(failure) = self.guard.adopt(pid) {
                let _ = child.kill();
                return Err(Error::Terminal(format!(
                    "could not put the terminal under process reclamation: {failure}"
                )));
            }
        }

        let reader = pair.master.try_clone_reader().map_err(|failure| {
            Error::Terminal(format!("could not read the terminal: {failure}"))
        })?;
        let writer = pair.master.take_writer().map_err(|failure| {
            Error::Terminal(format!("could not write to the terminal: {failure}"))
        })?;

        // Split off the ability to signal the shell before the child itself goes
        // to the waiter thread, which will be blocked inside `wait` and unable to
        // lend it back. This split is the whole reason `ChildKiller` is a separate
        // trait.
        let killer = child.clone_killer();
        let (input, queued) = mpsc::channel::<Vec<u8>>();

        let id = format!("t{}", self.next.fetch_add(1, Ordering::Relaxed));
        let describe = Session {
            id: id.clone(),
            label: shell::label(&spec.path),
            cwd: spec.cwd,
        };

        // Registered before the threads start, so a shell that exits immediately
        // cannot be forgotten before it was ever remembered.
        self.live.lock().expect("terminals poisoned").insert(
            id.clone(),
            Live {
                master: pair.master,
                input,
                killer,
                describe: describe.clone(),
            },
        );

        // The first bytes belong to the shell's own banner, before the prompt.
        // Emitted through the normal channel, so a frontend that has not adopted
        // this id yet still buffers them in order.
        emit(Event::Output {
            id: id.clone(),
            data: spec.banner,
        });

        spawn_reader(emit.clone(), id.clone(), reader);
        spawn_writer(queued, writer);
        spawn_waiter(Arc::clone(self), emit, id, child);

        Ok(describe)
    }

    /// Send keystrokes to a terminal.
    ///
    /// Queued rather than written here: a shell that has stopped reading would
    /// otherwise block the command, and through it whichever runtime worker was
    /// unlucky enough to be running it.
    pub fn write(&self, id: &str, data: &str) -> Result<()> {
        let live = self.live.lock().expect("terminals poisoned");
        let session = live.get(id).ok_or_else(|| Self::gone(id))?;
        session
            .input
            .send(data.as_bytes().to_vec())
            .map_err(|_| Self::gone(id))
    }

    /// Tell the shell the pane changed size.
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<()> {
        let live = self.live.lock().expect("terminals poisoned");
        let session = live.get(id).ok_or_else(|| Self::gone(id))?;
        session
            .master
            .resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|failure| Error::Terminal(format!("could not resize the terminal: {failure}")))
    }

    /// End a terminal now.
    ///
    /// The session is not removed here. The shell's death is what removes it, via
    /// the waiter thread, so a terminal that closes because the user asked and one
    /// that closes because someone typed `exit` take exactly the same path — and
    /// the tab hears about both the same way.
    pub fn close(&self, id: &str) -> Result<()> {
        let mut live = self.live.lock().expect("terminals poisoned");
        let session = live.get_mut(id).ok_or_else(|| Self::gone(id))?;
        if let Err(failure) = session.killer.kill() {
            // Windows reports a successful termination as "os error 0" through
            // the pty layer — an error whose code is success is success.
            if failure.raw_os_error() != Some(0) {
                return Err(Error::Terminal(format!(
                    "could not end the terminal: {failure}"
                )));
            }
        }
        Ok(())
    }

    /// Every open terminal, so a pane that remounted can rebuild its tabs.
    pub fn list(&self) -> Vec<Session> {
        self.live
            .lock()
            .expect("terminals poisoned")
            .values()
            .map(|session| session.describe.clone())
            .collect()
    }

    /// Drop a session, having first let go of the lock.
    ///
    /// The order matters: dropping `master` closes the pseudo-console, and on
    /// Windows that blocks until the console host has flushed what it still holds.
    /// Doing it under the lock would stall every other terminal for as long as
    /// that takes.
    fn forget(&self, id: &str) {
        let session = self.live.lock().expect("terminals poisoned").remove(id);
        drop(session);
    }

    fn gone(id: &str) -> Error {
        Error::Terminal(format!("terminal {id} is no longer open"))
    }
}

/// One thread, reading the shell for as long as it lives.
fn spawn_reader(emit: Emit, id: String, mut reader: Box<dyn Read + Send>) {
    std::thread::Builder::new()
        .name(format!("terminal-reader-{id}"))
        .spawn(move || {
            let mut buffer = vec![0u8; READ_CHUNK];
            let mut decoder = Decoder::new();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        let text = decoder.feed(&buffer[..read]);
                        if !text.is_empty() {
                            emit(Event::Output { id: id.clone(), data: text });
                        }
                    }
                    Err(failure) if failure.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            // Whatever the pty flushed on close still belongs to the transcript.
            let tail = decoder.finish();
            if !tail.is_empty() {
                emit(Event::Output { id, data: tail });
            }
        })
        .expect("spawning a terminal reader");
}

/// One thread, writing whatever the user typed, in order.
fn spawn_writer(queued: mpsc::Receiver<Vec<u8>>, mut writer: Box<dyn Write + Send>) {
    std::thread::Builder::new()
        .name("terminal-writer".into())
        .spawn(move || {
            for chunk in queued {
                if writer.write_all(&chunk).is_err() {
                    break;
                }
                let _ = writer.flush();
            }
        })
        .expect("spawning a terminal writer");
}

/// One thread, turning a dead shell into an exit event and a dropped session.
///
/// The registry `Arc` moves in here on purpose: the session is removed and the
/// exit is announced before the last reference — and with it the Job Object the
/// shells live under — can go away.
fn spawn_waiter(
    terminals: Arc<Terminals>,
    emit: Emit,
    id: String,
    mut child: Box<dyn Child + Send + Sync>,
) {
    std::thread::Builder::new()
        .name(format!("terminal-waiter-{id}"))
        .spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code());
            // Remove the session before announcing the exit, so a `list` that
            // races the event never shows a terminal that is already gone.
            terminals.forget(&id);
            emit(Event::Exit {
                id,
                code: code.map(u32::from),
            });
        })
        .expect("spawning a terminal waiter");
}

#[cfg(test)]
mod tests {
    use super::{OpenSpec, Terminals};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// A real shell, opened for real: the one terminal test that proves the
    /// pty, the threads and the reclamation all agree.
    #[test]
    fn a_shell_opens_echos_and_is_reclaimed() {
        let terminals = Terminals::new().expect("terminal registry");
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&seen);

        let session = terminals
            .open(
                OpenSpec {
                    rows: 24,
                    cols: 80,
                    cwd: std::env::temp_dir(),
                    banner: String::new(),
                    path: std::env::var_os("PATH").unwrap_or_default(),
                    environment: HashMap::new(),
                },
                Arc::new(move |event| match event {
                    crate::terminal::Event::Output { data, .. } => {
                        sink.lock().expect("sink").push(data)
                    }
                    crate::terminal::Event::Exit { .. } => {}
                }),
            )
            .expect("a shell");

        terminals
            // Plain `echo`, spelled so that cmd, PowerShell and sh all answer
            // with the same text: this test is about the pty plumbing, not
            // about which shell won the lookup.
            .write(&session.id, "echo harnesslite-terminal-42\r\n")
            .expect("writing a line");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        // The shell asks where the cursor is (DSR 6n) before it draws anything,
        // and waits for the answer. A real frontend is an emulator that replies
        // `ESC[<row>;<col>R`; the test plays that role, because the plumbing
        // under test is the pty, not the emulator.
        let mut answered_cursor_query = false;
        loop {
            let transcript = seen.lock().expect("sink").join("");
            if transcript.contains("harnesslite-terminal-42") {
                break;
            }
            if !answered_cursor_query && transcript.contains("\u{1b}[6n") {
                answered_cursor_query = true;
                terminals
                    .write(&session.id, "\u{1b}[1;1R")
                    .expect("answering the cursor query");
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the shell never echoed; captured so far: {transcript:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Closing is confirmed by the session disappearing, not by the call
        // returning: the waiter thread owns that transition for every path.
        terminals.close(&session.id).expect("closing the shell");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if terminals.list().is_empty() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the closed shell was never reclaimed"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

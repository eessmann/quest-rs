use crate::{Client, Error};
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions, kill_process_group, waitid};
use std::{
    io::{Read, Write},
    os::unix::process::CommandExt,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

struct ChildGuard {
    child: Option<Child>,
}
impl ChildGuard {
    const fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }
    fn child_mut(&mut self) -> std::io::Result<&mut Child> {
        self.child
            .as_mut()
            .ok_or_else(|| std::io::Error::other("worker already reaped"))
    }
    fn exited(&mut self) -> std::io::Result<bool> {
        // NOWAIT keeps the exited leader's PID reserved until group cleanup.
        let pid = Pid::from_child(self.child_mut()?);
        Ok(waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        )?
        .is_some())
    }
    fn finish(&mut self) -> std::io::Result<ExitStatus> {
        let child = self.child_mut()?;
        let _ = kill_process_group(Pid::from_child(child), Signal::KILL);
        let status = child.wait()?;
        // Never signal a numeric process-group ID after its leader was reaped.
        self.child = None;
        Ok(status)
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            // The leader is still owned and unreaped, even after observed exit.
            let _ = kill_process_group(Pid::from_child(child), Signal::KILL);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
enum Event {
    Output(Vec<u8>),
    Stderr(Vec<u8>),
    Written,
    Error(Error),
}
fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, Error> {
    let capacity = limit.checked_add(1).ok_or(Error::Limits)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(std::io::Error::other)?;
    reader
        .take(u64::try_from(capacity).map_err(|_| Error::Limits)?)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::OutputLimit);
    }
    Ok(bytes)
}
struct Progress {
    stdout: Option<Vec<u8>>,
    stderr: Option<Vec<u8>>,
    written: bool,
}
impl Progress {
    fn accept(&mut self, event: Event) -> Result<(), Error> {
        match event {
            Event::Output(bytes) => self.stdout = Some(bytes),
            Event::Stderr(bytes) => self.stderr = Some(bytes),
            Event::Written => self.written = true,
            Event::Error(error) => return Err(error),
        }
        Ok(())
    }
    fn finish(self, status: ExitStatus) -> Result<Vec<u8>, Error> {
        if !status.success() {
            return Err(Error::Failed {
                status: status.to_string(),
                stderr: String::from_utf8_lossy(&self.stderr.unwrap_or_default()).into_owned(),
            });
        }
        self.stdout.ok_or(Error::Unexpected)
    }
}
fn watch(
    child: &mut ChildGuard,
    events: &Receiver<Event>,
    timeout: Duration,
) -> Result<Vec<u8>, Error> {
    let started = Instant::now();
    let mut progress = Progress {
        stdout: None,
        stderr: None,
        written: false,
    };
    let mut exited = false;
    loop {
        while let Ok(event) = events.try_recv() {
            progress.accept(event)?;
        }
        if !exited {
            exited = child.exited()?;
        }
        if exited && progress.stdout.is_some() && progress.stderr.is_some() && progress.written {
            return progress.finish(child.finish()?);
        }
        if started.elapsed() >= timeout {
            return Err(Error::Timeout);
        }
        match events.recv_timeout(Duration::from_millis(5)) {
            Ok(event) => progress.accept(event)?,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if exited {
                    return progress.finish(child.finish()?);
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
impl Client {
    pub(super) fn run(&self, bytes: Vec<u8>) -> Result<Vec<u8>, Error> {
        let prlimit = std::path::Path::new("/usr/bin/prlimit");
        if !prlimit.is_file() {
            return Err(Error::Capability("/usr/bin/prlimit missing"));
        }
        let executable = self.executable.canonicalize()?;
        let cpu_seconds = self.limits.wall_time.as_secs().saturating_add(1).min(30);
        let mut child = ChildGuard::new(
            Command::new(prlimit)
                .arg(format!("--as={0}:{0}", self.limits.memory_bytes))
                .arg(format!("--cpu={cpu_seconds}:{cpu_seconds}"))
                .arg("--")
                .arg(executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0)
                .spawn()?,
        );
        let stdout = child.child_mut()?.stdout.take().ok_or(Error::Unexpected)?;
        let stderr = child.child_mut()?.stderr.take().ok_or(Error::Unexpected)?;
        let mut stdin = child.child_mut()?.stdin.take().ok_or(Error::Unexpected)?;
        let (sender, receiver) = mpsc::channel();
        let output_sender = sender.clone();
        let error_sender = sender.clone();
        let limit = self.limits.output_bytes;
        thread::Builder::new()
            .name("quest-worker-output".into())
            .spawn(move || {
                let event = read_bounded(stdout, limit).map_or_else(Event::Error, Event::Output);
                let _ = output_sender.send(event);
            })?;
        thread::Builder::new()
            .name("quest-worker-stderr".into())
            .spawn(move || {
                let event = read_bounded(stderr, limit).map_or_else(Event::Error, Event::Stderr);
                let _ = error_sender.send(event);
            })?;
        thread::Builder::new()
            .name("quest-worker-input".into())
            .spawn(move || {
                // Early child exit is diagnosed by its status/output, not a masked broken pipe.
                let event = match stdin.write_all(&bytes) {
                    Ok(()) => Event::Written,
                    Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Event::Written,
                    Err(error) => Event::Error(Error::Io(error)),
                };
                drop(stdin);
                let _ = sender.send(event);
            })?;
        watch(&mut child, &receiver, self.limits.wall_time)
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use googletest::{Result, prelude::*};
    #[gtest]
    fn exit_observation_keeps_pid_owned_until_group_cleanup_and_reap() -> Result<()> {
        let child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()?;
        let mut guard = ChildGuard::new(child);
        let deadline = Instant::now();
        while !guard.exited()? {
            if deadline.elapsed() > Duration::from_secs(2) {
                return Err(std::io::Error::other("fixture child did not exit").into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        let pid = Pid::from_child(guard.child_mut()?);
        expect_true!(
            rustix::process::waitid(
                rustix::process::WaitId::Pid(pid),
                rustix::process::WaitIdOptions::EXITED
                    | rustix::process::WaitIdOptions::NOWAIT
                    | rustix::process::WaitIdOptions::NOHANG
            )?
            .is_some()
        );
        expect_true!(guard.finish()?.success());
        expect_true!(guard.child.is_none());
        Ok(())
    }
}

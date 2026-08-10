use super::env::{CommandEnv, CommandEnvs, CommandResolvedEnvs};
pub use crate::ffi::OsString as EnvKey;
use crate::ffi::{OsStr, OsString};
use crate::num::NonZero;
use crate::path::Path;
use crate::process::StdioPipes;
use crate::sys::fs::File;
use crate::sys::pipe::Pipe;
use crate::{fmt, io};

////////////////////////////////////////////////////////////////////////////////
// Command
////////////////////////////////////////////////////////////////////////////////

pub struct Command {
    program: OsString,
    args: Vec<OsString>,
    env: CommandEnv,

    cwd: Option<OsString>,
    stdin: Option<Stdio>,
    stdout: Option<Stdio>,
    stderr: Option<Stdio>,
    extra_fds: Vec<[u32; 2]>,
    endowments: Vec<(String, u32)>,
    provided: Vec<(String, u32)>,
}

#[derive(Debug)]
pub enum Stdio {
    Inherit,
    Null,
    MakePipe,
    MakeTtyPipe,
    ParentStdout,
    ParentStderr,
    InheritFile(File),
    InheritPipe(Pipe),
}

impl Command {
    pub fn new(program: &OsStr) -> Command {
        Command {
            program: program.to_owned(),
            args: vec![program.to_owned()],
            env: Default::default(),
            cwd: None,
            stdin: None,
            stdout: None,
            stderr: None,
            extra_fds: Vec::new(),
            endowments: Vec::new(),
            provided: Vec::new(),
        }
    }

    pub fn arg(&mut self, arg: &OsStr) {
        self.args.push(arg.to_owned());
    }

    pub fn env_mut(&mut self) -> &mut CommandEnv {
        &mut self.env
    }

    pub fn cwd(&mut self, dir: &OsStr) {
        self.cwd = Some(dir.to_owned());
    }

    pub fn stdin(&mut self, stdin: Stdio) {
        self.stdin = Some(stdin);
    }

    pub fn stdout(&mut self, stdout: Stdio) {
        self.stdout = Some(stdout);
    }

    pub fn stderr(&mut self, stderr: Stdio) {
        self.stderr = Some(stderr);
    }

    pub fn get_program(&self) -> &OsStr {
        &self.program
    }

    pub fn get_args(&self) -> CommandArgs<'_> {
        let mut iter = self.args.iter();
        iter.next();
        CommandArgs { iter }
    }

    pub fn get_envs(&self) -> CommandEnvs<'_> {
        self.env.iter()
    }

    pub fn get_env_clear(&self) -> bool {
        self.env.does_clear()
    }

    pub fn get_resolved_envs(&self) -> CommandResolvedEnvs {
        CommandResolvedEnvs::new(self.env.capture())
    }

    pub fn get_current_dir(&self) -> Option<&Path> {
        self.cwd.as_ref().map(|cs| Path::new(cs))
    }

    /// Add an extra file descriptor mapping for the child process.
    /// The child will see `child_fd` mapped to the parent's `parent_fd`.
    pub fn inherit_fd(&mut self, child_fd: u32, parent_fd: u32) {
        self.extra_fds.push([child_fd, parent_fd]);
    }

    /// Give the child `handle` under the name `label`.
    ///
    /// The handle is **moved**: after a successful spawn this process no longer
    /// holds it. A caller that wants to keep one duplicates it first.
    pub fn endow(&mut self, label: &str, handle: u32) {
        self.endowments.push((label.to_owned(), handle));
    }

    /// Give the child `connector` under `name`, on top of its manifest row.
    ///
    /// Routes the spawn through the launcher, which is the only thing that can
    /// build a child's authority out of the child's own declaration.
    pub fn provide(&mut self, name: &str, connector: u32) {
        self.provided.push((name.to_owned(), connector));
    }

    /// A duplicate of this process's own namespace handle, for the child to be
    /// endowed under `svc`.
    ///
    /// `None` when the caller endowed one itself — it has decided what its
    /// child may reach — and when this process has no namespace, which is a
    /// program the manifest gives no `receives` and whose children therefore
    /// have nothing to inherit.
    fn inherited_namespace(&self) -> Option<toyos_abi::RawHandle> {
        if self.endowments.iter().any(|(label, _)| label == toyos_abi::syscall::SVC_LABEL) {
            return None;
        }
        let namespace = toyos::endow::namespace()?;
        toyos_abi::syscall::dup(toyos::AsHandle::as_handle(namespace)).ok()
    }

    fn resolve_program(&self) -> io::Result<OsString> {
        let prog = self.program.to_str().unwrap_or("");
        if prog.contains('/') {
            return Ok(self.program.clone());
        }
        // Search PATH for the executable
        if let Some(path_var) = crate::env::var_os("PATH") {
            for dir in crate::env::split_paths(&path_var) {
                let candidate = dir.join(prog);
                if candidate.exists() {
                    return Ok(candidate.into_os_string());
                }
            }
        }
        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    pub fn spawn(
        &mut self,
        default: Stdio,
        _needs_stdin: bool,
    ) -> io::Result<(Process, StdioPipes)> {
        let resolved = self.resolve_program()?;
        let mut argv_buf = Vec::new();
        argv_buf.extend_from_slice(resolved.as_encoded_bytes());
        for arg in &self.args[1..] {
            argv_buf.push(0);
            argv_buf.extend_from_slice(arg.as_encoded_bytes());
        }

        let stdin = self.stdin.as_ref().unwrap_or(&default);
        let stdout = self.stdout.as_ref().unwrap_or(&default);
        let stderr = self.stderr.as_ref().unwrap_or(&default);

        let mut fd_map: Vec<[u32; 2]> = Vec::new();
        let mut child_pipes: Vec<Pipe> = Vec::new();
        let mut stdin_pipe: Option<Pipe> = None;
        let mut stdout_pipe: Option<Pipe> = None;
        let mut stderr_pipe: Option<Pipe> = None;

        // Resolve each stdio to an fd_map entry: [child_fd, parent_fd]
        Self::setup_fd(&mut fd_map, &mut child_pipes, &mut stdin_pipe, stdin, 0, true)?;
        Self::setup_fd(&mut fd_map, &mut child_pipes, &mut stdout_pipe, stdout, 1, false)?;
        Self::setup_fd(&mut fd_map, &mut child_pipes, &mut stderr_pipe, stderr, 2, false)?;

        // Add extra fd mappings (e.g., for jobserver pipes)
        fd_map.extend_from_slice(&self.extra_fds);

        // Build environment: serialize all env vars as KEY=VALUE\0KEY2=VALUE2\0...
        let mut env_buf = Vec::new();
        let capture = self.env.capture();
        for (key, value) in capture.iter() {
            env_buf.extend_from_slice(key.as_encoded_bytes());
            env_buf.push(b'=');
            env_buf.extend_from_slice(value.as_encoded_bytes());
            env_buf.push(0);
        }

        // The label blob and the entries that index it. Built here because the
        // kernel reads both out of one call and keeps the blob for the child's
        // life.
        let mut labels = Vec::new();
        let mut endow = Vec::with_capacity(self.endowments.len() + 1);
        let mut push = |label: &str, handle: u32, labels: &mut Vec<u8>| {
            endow.push(toyos_abi::syscall::EndowEntry {
                label_off: labels.len() as u32,
                label_len: label.len() as u32,
                handle: toyos_abi::RawHandle(handle),
                _pad: 0,
            });
            labels.extend_from_slice(label.as_bytes());
        };
        for (label, handle) in &self.endowments {
            push(label, *handle, &mut labels);
        }
        // The child inherits this process's namespace unless the caller decided
        // otherwise. A duplicate rather than the handle itself: an endowment is
        // a move, and a parent that gave its namespace away could not spawn a
        // second child. A caller that endows `svc` has decided what its child
        // may reach and is not overruled here.
        let inherited = self.inherited_namespace();
        if let Some(handle) = inherited {
            push(toyos_abi::syscall::SVC_LABEL, handle.0, &mut labels);
        }

        // **The routing rule (`specs/capability-endowment-spec.md` §4.5).**
        // A caller that endowed a handle or named an extra fd has decided what
        // its child holds, and the launcher would overwrite that decision with
        // a manifest row — so those spawn directly. Everything else asks the
        // launcher when it holds one, and falls back for a program the image
        // does not declare. A caller with no `launcher` connector gets plain
        // inheritance, which is what a program endowed nothing should get.
        let decided = !self.endowments.is_empty() || !self.extra_fds.is_empty();
        if !decided {
            if let Some(process) = self.launch(&resolved, &argv_buf, &env_buf, &fd_map)? {
                drop(child_pipes);
                return Ok((
                    process,
                    StdioPipes { stdin: stdin_pipe, stdout: stdout_pipe, stderr: stderr_pipe },
                ));
            }
        }

        let spawn_args = toyos_abi::syscall::SpawnArgs {
            argv_ptr: argv_buf.as_ptr().expose_provenance() as u64,
            argv_len: argv_buf.len() as u64,
            slot_map_ptr: fd_map.as_ptr().expose_provenance() as u64,
            slot_map_count: fd_map.len() as u64,
            env_ptr: env_buf.as_ptr().expose_provenance() as u64,
            env_len: env_buf.len() as u64,
            endow_ptr: endow.as_ptr().expose_provenance() as u64,
            endow_count: endow.len() as u64,
            labels_ptr: labels.as_ptr().expose_provenance() as u64,
            labels_len: labels.len() as u64,
        };
        // SAFETY: spawn_args contains valid pointers to stack-local buffers that outlive the call.
        let spawned = unsafe { toyos_abi::syscall::spawn(&spawn_args) };

        // Close child-side pipe ends in the parent
        drop(child_pipes);

        let handle = spawned.map_err(|e| {
            // An endowment moves only on a spawn that happened, so the
            // duplicate this call made is still ours and is ours to close.
            if let Some(handle) = inherited {
                toyos_abi::syscall::close(handle);
            }
            let kind = match e {
                toyos_abi::syscall::SyscallError::NotFound => io::ErrorKind::NotFound,
                _ => io::ErrorKind::Other,
            };
            io::Error::from(kind)
        })?;

        Ok((
            // SAFETY: the kernel installed this handle in our table for this
            // call and no other.
            Process { handle: unsafe { toyos::process::Process::from_raw(handle) } },
            StdioPipes {
                stdin: stdin_pipe,
                stdout: stdout_pipe,
                stderr: stderr_pipe,
            },
        ))
    }

    /// Ask `/bin/init` to start this program, or answer `None` for a caller
    /// that cannot or a program the manifest does not declare.
    ///
    /// The stdio handles are **duplicated** before they go: a launch moves what
    /// it carries, and `Stdio::Inherit` names the parent's own slot 1.
    fn launch(
        &self,
        resolved: &OsStr,
        argv: &[u8],
        env: &[u8],
        fd_map: &[[u32; 2]],
    ) -> io::Result<Option<Process>> {
        use toyos::launch::{
            self, Launch, LaunchError, Outcome, MAX_LAUNCH_EXTRAS, MAX_LAUNCH_SLOTS,
        };

        // A `provide` is a statement that the child's authority comes from its
        // own manifest row plus this connector, and only the launcher can build
        // that. Without one there is no weaker thing to fall back to that would
        // still be what the caller asked for.
        let Ok(conn) = toyos::endow::service("launcher") else {
            return if self.provided.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            };
        };

        // The whole path, not a key: `/bin/ls` is a symlink to `/bin/toybox`
        // and the row that says what an applet holds is `toybox`'s. Resolving
        // that is init's — it holds the manifest and this process must not
        // become a second reader of it.
        let program = resolved.to_str().unwrap_or("");

        if self.provided.len() > MAX_LAUNCH_EXTRAS || fd_map.len() > MAX_LAUNCH_SLOTS {
            return Ok(None);
        }

        let mut slots: Vec<(u32, toyos_abi::RawHandle)> = Vec::with_capacity(fd_map.len());
        for &[child_slot, parent] in fd_map {
            match toyos_abi::syscall::dup(toyos_abi::RawHandle(parent)) {
                Ok(copy) => slots.push((child_slot, copy)),
                Err(_) => {
                    for (_, h) in &slots {
                        toyos_abi::syscall::close(*h);
                    }
                    return Ok(None);
                }
            }
        }
        let extras: Vec<(&str, toyos_abi::RawHandle)> = self
            .provided
            .iter()
            .map(|(name, handle)| (name.as_str(), toyos_abi::RawHandle(*handle)))
            .collect();
        let cwd = self
            .cwd
            .as_ref()
            .and_then(|c| c.to_str())
            .map(String::from)
            .unwrap_or_else(|| {
                crate::env::current_dir()
                    .ok()
                    .and_then(|p| p.to_str().map(String::from))
                    .unwrap_or_else(|| String::from("/"))
            });

        let request = Launch { program, argv, env, cwd: &cwd, extras: &extras, slots: &slots };
        let answer = launch::launch(&conn, &request);

        // **The send moved them.** Every arm below but `NotSent` is past the
        // point where these duplicates left this table, so closing them here
        // would be this process naming a handle it does not hold — which the
        // kernel answers by ending it. The launcher releases what it took.
        match answer {
            Ok(Outcome::Started(handle)) => {
                // SAFETY: init moved this handle into our table and holds none.
                Ok(Some(Process { handle: unsafe { toyos::process::Process::from_raw(handle) } }))
            }
            // The direct path, which is what §4.5 clause 2 says an undeclared
            // program gets. A caller that transferred connectors loses nothing
            // by it: init merges a launched program's extras into the namespace
            // it builds, so a caller that was itself launched already carries
            // them, and the child inherits that. What the direct path cannot do
            // is merge a name into an *inherited* namespace — no caller in the
            // tree needs it, and
            // `specs/issues/isolation/a-provided-name-cannot-reach-an-undeclared-child.md`
            // is where that is written down.
            Ok(Outcome::NotDeclared) => Ok(None),
            Ok(Outcome::Refused) | Err(LaunchError::Sent(_)) => {
                Err(io::Error::from(io::ErrorKind::Other))
            }
            Err(LaunchError::NotSent(_)) => {
                for (_, h) in &slots {
                    toyos_abi::syscall::close(*h);
                }
                Ok(None)
            }
        }
    }

    fn setup_fd(
        fd_map: &mut Vec<[u32; 2]>,
        child_pipes: &mut Vec<Pipe>,
        parent_pipe: &mut Option<Pipe>,
        stdio: &Stdio,
        child_fd: u32,
        is_input: bool,
    ) -> io::Result<()> {
        match stdio {
            Stdio::Inherit => fd_map.push([child_fd, child_fd]),
            Stdio::MakePipe | Stdio::MakeTtyPipe => {
                let (r, w) = crate::sys::pipe::pipe()?;
                if matches!(stdio, Stdio::MakeTtyPipe) {
                    toyos_abi::syscall::mark_tty(toyos_abi::RawHandle(r.raw_fd() as u32));
                    toyos_abi::syscall::mark_tty(toyos_abi::RawHandle(w.raw_fd() as u32));
                }
                if is_input {
                    fd_map.push([child_fd, r.raw_fd() as u32]);
                    child_pipes.push(r);
                    *parent_pipe = Some(w);
                } else {
                    fd_map.push([child_fd, w.raw_fd() as u32]);
                    child_pipes.push(w);
                    *parent_pipe = Some(r);
                }
            }
            Stdio::InheritFile(file) => fd_map.push([child_fd, file.raw_fd() as u32]),
            Stdio::InheritPipe(pipe) => fd_map.push([child_fd, pipe.raw_fd() as u32]),
            Stdio::ParentStdout => fd_map.push([child_fd, 1]),
            Stdio::ParentStderr => fd_map.push([child_fd, 2]),
            Stdio::Null => {}
        }
        Ok(())
    }
}

pub fn output(cmd: &mut Command) -> io::Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let (mut process, pipes) = cmd.spawn(Stdio::MakePipe, false)?;
    let mut stdout_data = Vec::new();
    if let Some(pipe) = pipes.stdout {
        pipe.read_to_end(&mut stdout_data)?;
    }
    let status = process.wait()?;
    Ok((status, stdout_data, Vec::new()))
}

impl From<ChildPipe> for Stdio {
    fn from(pipe: ChildPipe) -> Stdio {
        Stdio::InheritPipe(pipe)
    }
}

impl From<io::Stdout> for Stdio {
    fn from(_: io::Stdout) -> Stdio {
        Stdio::ParentStdout
    }
}

impl From<io::Stderr> for Stdio {
    fn from(_: io::Stderr) -> Stdio {
        Stdio::ParentStderr
    }
}

impl From<File> for Stdio {
    fn from(file: File) -> Stdio {
        Stdio::InheritFile(file)
    }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            let mut debug_command = f.debug_struct("Command");
            debug_command.field("program", &self.program).field("args", &self.args);
            if !self.env.is_unchanged() {
                debug_command.field("env", &self.env);
            }

            if self.cwd.is_some() {
                debug_command.field("cwd", &self.cwd);
            }

            if self.stdin.is_some() {
                debug_command.field("stdin", &self.stdin);
            }
            if self.stdout.is_some() {
                debug_command.field("stdout", &self.stdout);
            }
            if self.stderr.is_some() {
                debug_command.field("stderr", &self.stderr);
            }

            debug_command.finish()
        } else {
            if let Some(ref cwd) = self.cwd {
                write!(f, "cd {cwd:?} && ")?;
            }
            if self.env.does_clear() {
                write!(f, "env -i ")?;
            } else {
                let mut any_removed = false;
                for (key, value_opt) in self.get_envs() {
                    if value_opt.is_none() {
                        if !any_removed {
                            write!(f, "env ")?;
                            any_removed = true;
                        }
                        write!(f, "-u {} ", key.to_string_lossy())?;
                    }
                }
            }
            for (key, value_opt) in self.get_envs() {
                if let Some(value) = value_opt {
                    write!(f, "{}={value:?} ", key.to_string_lossy())?;
                }
            }
            if self.program != self.args[0] {
                write!(f, "[{:?}] ", self.program)?;
            }
            write!(f, "{:?}", self.args[0])?;

            for arg in &self.args[1..] {
                write!(f, " {:?}", arg)?;
            }
            Ok(())
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitStatus(i32);

impl Default for ExitStatus {
    fn default() -> Self {
        ExitStatus(0)
    }
}

impl ExitStatus {
    pub fn exit_ok(&self) -> Result<(), ExitStatusError> {
        if self.0 == 0 {
            Ok(())
        } else {
            Err(ExitStatusError(self.0))
        }
    }

    pub fn code(&self) -> Option<i32> {
        Some(self.0)
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exit status: {}", self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ExitStatusError(i32);

impl Into<ExitStatus> for ExitStatusError {
    fn into(self) -> ExitStatus {
        ExitStatus(self.0)
    }
}

impl ExitStatusError {
    pub fn code(self) -> Option<NonZero<i32>> {
        NonZero::new(self.0)
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ExitCode(u8);

impl ExitCode {
    pub const SUCCESS: ExitCode = ExitCode(0);
    pub const FAILURE: ExitCode = ExitCode(1);

    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
}

impl From<u8> for ExitCode {
    fn from(code: u8) -> Self {
        Self(code)
    }
}

/// A child, as the handle its spawn answered with.
///
/// There is no pid in here and no way back from one: what may wait for this
/// child, kill it or read its accounting is exactly what holds this handle.
pub struct Process {
    handle: toyos::process::Process,
}

impl Process {
    /// The child's pid, which is a name and not a key.
    ///
    /// Read out of the accounting record rather than kept beside the handle:
    /// this is a diagnostic and nothing in the tree calls it on a hot path, so
    /// paying a syscall for it is better than a second copy of the identity.
    /// Zero for a child whose accounting the kernel would not answer for, which
    /// is a process torn down between the spawn and the question.
    pub fn id(&self) -> u32 {
        self.handle.stats().map_or(0, |s| s.pid)
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.handle.kill().map_err(|_| io::Error::from(io::ErrorKind::Other))
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.handle
            .wait()
            .map(ExitStatus)
            .map_err(|_| io::Error::from(io::ErrorKind::Other))
    }

    /// See `os::toyos::process::ChildExt::as_raw_handle`.
    pub fn as_raw_handle(&self) -> u32 {
        toyos::AsHandle::as_handle(&self.handle).0
    }

    /// Give up the handle. See `os::toyos::process::ChildExt::into_raw_handle`.
    pub fn into_raw_handle(self) -> u32 {
        self.handle.into_raw().0
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match self.handle.try_wait() {
            Ok(code) => Ok(Some(ExitStatus(code))),
            Err(toyos_abi::syscall::SyscallError::WouldBlock) => Ok(None),
            Err(_) => Err(io::Error::from(io::ErrorKind::Other)),
        }
    }
}

pub struct CommandArgs<'a> {
    iter: crate::slice::Iter<'a, OsString>,
}

impl<'a> Iterator for CommandArgs<'a> {
    type Item = &'a OsStr;
    fn next(&mut self) -> Option<&'a OsStr> {
        self.iter.next().map(|os| &**os)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a> ExactSizeIterator for CommandArgs<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
    fn is_empty(&self) -> bool {
        self.iter.is_empty()
    }
}

impl<'a> fmt::Debug for CommandArgs<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter.clone()).finish()
    }
}

pub type ChildPipe = Pipe;

pub fn getpid() -> u32 {
    toyos_abi::syscall::getpid().0
}

pub fn read_output(
    out: ChildPipe,
    stdout: &mut Vec<u8>,
    err: ChildPipe,
    stderr: &mut Vec<u8>,
) -> io::Result<()> {
    // Read both pipes concurrently to avoid deadlock: if the child fills one
    // pipe buffer while we're blocking on the other, both sides stall.
    use crate::thread;
    let err_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        err.read_to_end(&mut buf).map(|_| buf)
    });
    out.read_to_end(stdout)?;
    match err_thread.join() {
        Ok(Ok(buf)) => { *stderr = buf; Ok(()) }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(io::Error::new(io::ErrorKind::Other, "stderr reader thread panicked")),
    }
}

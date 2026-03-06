use super::env::{CommandEnv, CommandEnvs};
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

    pub fn get_current_dir(&self) -> Option<&Path> {
        self.cwd.as_ref().map(|cs| Path::new(cs))
    }

    pub fn spawn(
        &mut self,
        default: Stdio,
        _needs_stdin: bool,
    ) -> io::Result<(Process, StdioPipes)> {
        let mut argv_buf = Vec::new();
        argv_buf.extend_from_slice(self.program.as_encoded_bytes());
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

        let pid = toyos_abi::syscall::spawn(&argv_buf, &fd_map);

        // Close child-side pipe ends in the parent
        drop(child_pipes);

        let pid = pid.map_err(|e| {
            let kind = match e {
                toyos_abi::syscall::SyscallError::NotFound => io::ErrorKind::NotFound,
                _ => io::ErrorKind::Other,
            };
            io::Error::from(kind)
        })?;

        Ok((
            Process { pid: pid.0 },
            StdioPipes {
                stdin: stdin_pipe,
                stdout: stdout_pipe,
                stderr: stderr_pipe,
            },
        ))
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
                    toyos_abi::syscall::mark_tty(toyos_abi::syscall::Fd(r.raw_fd()));
                    toyos_abi::syscall::mark_tty(toyos_abi::syscall::Fd(w.raw_fd()));
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

pub struct Process {
    pid: u32,
}

impl Process {
    pub fn id(&self) -> u32 {
        self.pid
    }

    pub fn kill(&mut self) -> io::Result<()> {
        panic!("Process::kill not supported on ToyOS");
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        let code = toyos_abi::syscall::waitpid(toyos_abi::syscall::Pid(self.pid));
        Ok(ExitStatus(code as i32))
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.wait().map(Some)
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
    _out: ChildPipe,
    _stdout: &mut Vec<u8>,
    _err: ChildPipe,
    _stderr: &mut Vec<u8>,
) -> io::Result<()> {
    panic!("read_output not supported on ToyOS (stderr is never piped)");
}

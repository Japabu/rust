use crate::sys::process as imp;
use crate::sys::{AsInner, AsInnerMut, FromInner, IntoInner};

/// Create a `Stdio` that pipes through a tty-typed file descriptor.
///
/// Like `Stdio::piped()`, but the pipe endpoints are marked as tty so the
/// child process gets canonical mode (echo + line editing) on its stdin.
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub fn tty_piped() -> crate::process::Stdio {
    crate::process::Stdio::from_inner(imp::Stdio::MakeTtyPipe)
}

/// ToyOS-specific extensions to [`process::Command`].
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub trait CommandExt {
    /// Pass an additional file descriptor to the child process.
    ///
    /// The child process will inherit `parent_fd` as `child_fd`.
    /// This is useful for passing pipe file descriptors (e.g., for jobserver
    /// protocols) to child processes.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn inherit_fd(&mut self, child_fd: u32, parent_fd: u32) -> &mut Self;

    /// Give the child a handle under a name it can look itself up by.
    ///
    /// The handle is **moved**: after a successful spawn the parent no longer
    /// holds it, which is what lets a capability that admits only one holder —
    /// a device claim — be handed over at all. A parent that wants to keep one
    /// duplicates it first.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn endow(&mut self, label: &str, handle: u32) -> &mut Self;

    /// Put `connector` in the child's namespace under `name`, on top of what
    /// the manifest says the child holds.
    ///
    /// **This is a launch, not a spawn.** A terminal's `surface` port exists
    /// once per terminal, so `/bin/init` cannot know it and the manifest cannot
    /// name it — but the shell's own `[programs]` row is what should decide the
    /// rest of what a shell holds. So the caller supplies this one connector,
    /// init supplies the row, and the child's namespace is the union.
    ///
    /// The connector is **moved**, like [`endow`](CommandExt::endow), and the
    /// spawn fails if this process holds no `launcher` connector: there is
    /// nowhere else the manifest row can come from.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn provide(&mut self, name: &str, connector: u32) -> &mut Self;
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl CommandExt for crate::process::Command {
    fn inherit_fd(&mut self, child_fd: u32, parent_fd: u32) -> &mut Self {
        self.as_inner_mut().inherit_fd(child_fd, parent_fd);
        self
    }

    fn endow(&mut self, label: &str, handle: u32) -> &mut Self {
        self.as_inner_mut().endow(label, handle);
        self
    }

    fn provide(&mut self, name: &str, connector: u32) -> &mut Self {
        self.as_inner_mut().provide(name, connector);
        self
    }
}

/// ToyOS-specific extensions to [`process::Child`].
#[stable(feature = "toyos_ext", since = "1.0.0")]
pub trait ChildExt {
    /// Give up this process's handle, for one about to be sent or endowed.
    ///
    /// After this the parent no longer holds the child: it cannot wait for it,
    /// kill it or read its accounting. `/bin/init`'s launcher is the caller —
    /// it answers with the handle and keeps none, because a process that could
    /// ask it to start `/bin/true` in a loop would otherwise exhaust the one
    /// handle table the whole machine depends on.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn into_raw_handle(self) -> u32;

    /// This child's process handle, without giving it up.
    ///
    /// **A number to pass to the ABI, not a second owner.** It is what a caller
    /// wanting more of a process than `wait` and `kill` — its accounting, a
    /// narrowed duplicate to hand on — reaches through, and it stays valid only
    /// while the `Child` is alive. std deliberately does not wrap those calls:
    /// their argument and answer types are `toyos-abi`'s, and a std signature
    /// naming them would drag every caller onto the sysroot's copy of that
    /// crate rather than its own.
    #[stable(feature = "toyos_ext", since = "1.0.0")]
    fn as_raw_handle(&self) -> u32;
}

#[stable(feature = "toyos_ext", since = "1.0.0")]
impl ChildExt for crate::process::Child {
    fn into_raw_handle(self) -> u32 {
        self.into_inner().into_raw_handle()
    }

    fn as_raw_handle(&self) -> u32 {
        self.as_inner().as_raw_handle()
    }
}

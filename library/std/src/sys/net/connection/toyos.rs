use crate::fmt;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, SocketAddrV4, ToSocketAddrs};
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use crate::sync::Arc;
use crate::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
use crate::time::Duration;
use toyos_abi::Fd;
use toyos::poller::{Poller, IORING_POLL_IN, IORING_POLL_OUT};
use toyos_abi::syscall::{self, SyscallError};
use toyos::net::{NetError, TcpSocketId, UdpSocketId};

// --- Helpers ---

fn net_err_to_io(e: NetError) -> io::Error {
    let kind = match e {
        NetError::ConnectionRefused => io::ErrorKind::ConnectionRefused,
        NetError::ConnectionReset => io::ErrorKind::ConnectionReset,
        NetError::TimedOut => io::ErrorKind::TimedOut,
        NetError::AddrInUse => io::ErrorKind::AddrInUse,
        NetError::NotConnected => io::ErrorKind::NotConnected,
        NetError::InvalidInput => io::ErrorKind::InvalidInput,
        NetError::NetdNotFound => io::ErrorKind::NotConnected,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, "netd error")
}

fn addr_to_v4(addr: &SocketAddr) -> io::Result<([u8; 4], u16)> {
    match addr {
        SocketAddr::V4(v4) => Ok((v4.ip().octets(), v4.port())),
        SocketAddr::V6(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPv6 not supported",
        )),
    }
}

fn duration_to_ms(d: Option<Duration>) -> u32 {
    match d {
        Some(d) => d.as_millis().min(u32::MAX as u128) as u32,
        None => 0,
    }
}

fn syscall_err(e: SyscallError) -> io::Error {
    match e {
        SyscallError::WouldBlock => io::ErrorKind::WouldBlock.into(),
        _ => io::Error::new(io::ErrorKind::Other, "syscall error"),
    }
}

/// Wrap rx/tx pipes into a single kernel socket fd.
fn make_socket_fd(rx: toyos::Pipe, tx: toyos::Pipe) -> io::Result<OwnedFd> {
    let rx_id = rx.pipe_id().map_err(syscall_err)?;
    let tx_id = tx.pipe_id().map_err(syscall_err)?;
    let socket_fd = syscall::socket_create(rx_id, tx_id).map_err(syscall_err)?;
    // Pipes are consumed — drop closes the underlying fds
    drop(rx);
    drop(tx);
    Ok(unsafe { OwnedFd::from_raw_fd(socket_fd.0) })
}

// --- Shared socket ownership (prevents double-close on duplicate) ---

enum NetdSocket {
    Tcp(TcpSocketId),
    Udp(UdpSocketId),
}

impl Drop for NetdSocket {
    fn drop(&mut self) {
        let _ = match self {
            NetdSocket::Tcp(id) => toyos::net::tcp_close(*id),
            NetdSocket::Udp(id) => toyos::net::udp_close(*id),
        };
    }
}

// --- TcpStream ---

pub struct TcpStream {
    fd: OwnedFd,
    socket: Arc<NetdSocket>,
    peer: SocketAddr,
    local_port: u16,
    read_timeout_ms: AtomicU32,
    write_timeout_ms: AtomicU32,
    nodelay: AtomicBool,
    nonblocking: AtomicBool,
}

impl TcpStream {
    fn socket_id(&self) -> TcpSocketId {
        match *self.socket {
            NetdSocket::Tcp(id) => id,
            _ => unreachable!(),
        }
    }

    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses found")
        })?;
        Self::connect_timeout(&addr, Duration::from_secs(30))
    }

    pub fn connect_timeout(addr: &SocketAddr, timeout: Duration) -> io::Result<TcpStream> {
        let (ip, port) = addr_to_v4(addr)?;
        let conn = toyos::net::tcp_connect(ip, port, duration_to_ms(Some(timeout)))
            .map_err(net_err_to_io)?;
        let fd = make_socket_fd(conn.rx, conn.tx)?;
        Ok(TcpStream {
            fd,
            socket: Arc::new(NetdSocket::Tcp(conn.socket_id)),
            peer: *addr,
            local_port: conn.local_port,
            read_timeout_ms: AtomicU32::new(0),
            write_timeout_ms: AtomicU32::new(0),
            nodelay: AtomicBool::new(false),
            nonblocking: AtomicBool::new(false),
        })
    }

    fn raw_fd(&self) -> Fd {
        Fd(self.fd.as_raw_fd())
    }

    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.read_timeout_ms.store(duration_to_ms(dur), Relaxed);
        Ok(())
    }

    pub fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.write_timeout_ms.store(duration_to_ms(dur), Relaxed);
        Ok(())
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        let ms = self.read_timeout_ms.load(Relaxed);
        Ok(if ms == 0 { None } else { Some(Duration::from_millis(ms as u64)) })
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        let ms = self.write_timeout_ms.load(Relaxed);
        Ok(if ms == 0 { None } else { Some(Duration::from_millis(ms as u64)) })
    }

    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "peek not supported"))
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.nonblocking.load(Relaxed) {
            return syscall::read_nonblock(self.raw_fd(), buf).map_err(syscall_err);
        }
        let timeout_ms = self.read_timeout_ms.load(Relaxed);
        if timeout_ms > 0 {
            let poller = Poller::new(1);
            poller.poll_add_fd(self.raw_fd(), IORING_POLL_IN, 0);
            let mut ready = false;
            poller.wait(1, timeout_ms as u64 * 1_000_000, |_| ready = true);
            if !ready {
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
        syscall::read(self.raw_fd(), buf).map_err(syscall_err)
    }

    pub fn read_buf(&self, mut buf: BorrowedCursor<'_, u8>) -> io::Result<()> {
        let mut tmp = vec![0u8; buf.capacity()];
        let n = self.read(&mut tmp)?;
        buf.append(&tmp[..n]);
        Ok(())
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        let mut total = 0;
        for buf in bufs {
            if buf.is_empty() {
                continue;
            }
            let n = self.read(buf)?;
            total += n;
            if n < buf.len() {
                break;
            }
        }
        Ok(total)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.nonblocking.load(Relaxed) {
            return syscall::write_nonblock(self.raw_fd(), buf).map_err(syscall_err);
        }
        let timeout_ms = self.write_timeout_ms.load(Relaxed);
        if timeout_ms > 0 {
            let poller = Poller::new(1);
            poller.poll_add_fd(self.raw_fd(), IORING_POLL_OUT, 0);
            let mut ready = false;
            poller.wait(1, timeout_ms as u64 * 1_000_000, |_| ready = true);
            if !ready {
                return Err(io::ErrorKind::TimedOut.into());
            }
        }
        syscall::write(self.raw_fd(), buf).map_err(syscall_err)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        let mut total = 0;
        for buf in bufs {
            if buf.is_empty() {
                continue;
            }
            let n = self.write(buf)?;
            total += n;
            if n < buf.len() {
                break;
            }
        }
        Ok(total)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.peer)
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        Ok(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(10, 0, 2, 15),
            self.local_port,
        )))
    }

    pub fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        let how_val = match how {
            Shutdown::Read => 0u32,
            Shutdown::Write => 1,
            Shutdown::Both => 2,
        };
        toyos::net::tcp_shutdown(self.socket_id(), how_val).map_err(net_err_to_io)
    }

    pub fn duplicate(&self) -> io::Result<TcpStream> {
        let new_fd = syscall::dup(self.raw_fd()).map_err(syscall_err)?;
        Ok(TcpStream {
            fd: unsafe { OwnedFd::from_raw_fd(new_fd.0) },
            socket: Arc::clone(&self.socket),
            peer: self.peer,
            local_port: self.local_port,
            read_timeout_ms: AtomicU32::new(self.read_timeout_ms.load(Relaxed)),
            write_timeout_ms: AtomicU32::new(self.write_timeout_ms.load(Relaxed)),
            nodelay: AtomicBool::new(self.nodelay.load(Relaxed)),
            nonblocking: AtomicBool::new(self.nonblocking.load(Relaxed)),
        })
    }

    pub fn set_linger(&self, _linger: Option<Duration>) -> io::Result<()> {
        Ok(())
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        Ok(None)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        toyos::net::tcp_set_option(self.socket_id(), toyos::net::OPT_NODELAY, nodelay as u32)
            .map_err(net_err_to_io)?;
        self.nodelay.store(nodelay, Relaxed);
        Ok(())
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        Ok(self.nodelay.load(Relaxed))
    }

    pub fn set_keepalive(&self, _keepalive: bool) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "keepalive not supported"))
    }

    pub fn keepalive(&self) -> io::Result<bool> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "keepalive not supported"))
    }

    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> {
        Ok(())
    }

    pub fn ttl(&self) -> io::Result<u32> {
        Ok(64)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.nonblocking.store(nonblocking, Relaxed);
        Ok(())
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    pub fn as_raw_fd(&self) -> i32 {
        self.fd.as_raw_fd()
    }
}

// No Drop impl — Arc<NetdSocket> handles close on last drop.
// OwnedFd drop closes the pipe-backed socket fd.

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TcpStream(fd={}, peer={})", self.fd.as_raw_fd(), self.peer)
    }
}

// --- TcpListener ---

pub struct TcpListener {
    notify_fd: OwnedFd,
    socket: Arc<NetdSocket>,
    local: SocketAddr,
    nonblocking: AtomicBool,
}

impl TcpListener {
    fn socket_id(&self) -> TcpSocketId {
        match *self.socket {
            NetdSocket::Tcp(id) => id,
            _ => unreachable!(),
        }
    }

    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses found")
        })?;
        let (ip, port) = addr_to_v4(&addr)?;
        let bound = toyos::net::tcp_bind(ip, port).map_err(net_err_to_io)?;
        Ok(TcpListener {
            notify_fd: unsafe { OwnedFd::from_raw_fd(bound.notify.into_fd().0) },
            socket: Arc::new(NetdSocket::Tcp(bound.socket_id)),
            local: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(ip), bound.bound_port)),
            nonblocking: AtomicBool::new(false),
        })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let mut byte = [0u8; 1];
        let notify_fd = Fd(self.notify_fd.as_raw_fd());
        if self.nonblocking.load(Relaxed) {
            syscall::read_nonblock(notify_fd, &mut byte).map_err(syscall_err)?;
        } else {
            syscall::read(notify_fd, &mut byte).map_err(syscall_err)?;
        }

        let accepted = toyos::net::tcp_accept(self.socket_id()).map_err(net_err_to_io)?;
        let fd = make_socket_fd(accepted.rx, accepted.tx)?;

        let peer = SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::from(accepted.remote_addr),
            accepted.remote_port,
        ));
        Ok((
            TcpStream {
                fd,
                socket: Arc::new(NetdSocket::Tcp(accepted.socket_id)),
                peer,
                local_port: accepted.local_port,
                read_timeout_ms: AtomicU32::new(0),
                write_timeout_ms: AtomicU32::new(0),
                nodelay: AtomicBool::new(false),
                nonblocking: AtomicBool::new(false),
            },
            peer,
        ))
    }

    pub fn duplicate(&self) -> io::Result<TcpListener> {
        let new_fd = syscall::dup(Fd(self.notify_fd.as_raw_fd())).map_err(syscall_err)?;
        Ok(TcpListener {
            notify_fd: unsafe { OwnedFd::from_raw_fd(new_fd.0) },
            socket: Arc::clone(&self.socket),
            local: self.local,
            nonblocking: AtomicBool::new(self.nonblocking.load(Relaxed)),
        })
    }

    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> {
        Ok(())
    }

    pub fn ttl(&self) -> io::Result<u32> {
        Ok(64)
    }

    pub fn set_only_v6(&self, _only_v6: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn only_v6(&self) -> io::Result<bool> {
        Ok(false)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.nonblocking.store(nonblocking, Relaxed);
        Ok(())
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.notify_fd.as_fd()
    }

    pub fn as_raw_fd(&self) -> i32 {
        self.notify_fd.as_raw_fd()
    }
}

// No Drop impl — Arc<NetdSocket> handles close on last drop.

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TcpListener(fd={}, local={})", self.notify_fd.as_raw_fd(), self.local)
    }
}

// --- UdpSocket ---

pub struct UdpSocket {
    socket: Arc<NetdSocket>,
    tx_fd: OwnedFd,
    rx_fd: OwnedFd,
    local: SocketAddr,
    peer: crate::sync::Mutex<Option<SocketAddr>>,
    read_timeout_ms: AtomicU32,
    write_timeout_ms: AtomicU32,
}

impl UdpSocket {
    fn socket_id(&self) -> UdpSocketId {
        match *self.socket {
            NetdSocket::Udp(id) => id,
            _ => unreachable!(),
        }
    }

    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses found")
        })?;
        let (ip, port) = addr_to_v4(&addr)?;
        let bound = toyos::net::udp_bind(ip, port).map_err(net_err_to_io)?;
        Ok(UdpSocket {
            socket: Arc::new(NetdSocket::Udp(bound.socket_id)),
            tx_fd: unsafe { OwnedFd::from_raw_fd(bound.tx.into_fd().0) },
            rx_fd: unsafe { OwnedFd::from_raw_fd(bound.rx.into_fd().0) },
            local: SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from(ip),
                bound.bound_port,
            )),
            peer: crate::sync::Mutex::new(None),
            read_timeout_ms: AtomicU32::new(0),
            write_timeout_ms: AtomicU32::new(0),
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.peer.lock().unwrap().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "not connected")
        })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let resp = toyos::net::udp_recv_from(self.socket_id(), buf.len() as u32)
            .map_err(net_err_to_io)?;

        let n = (resp.len as usize).min(buf.len());
        if n > 0 {
            let rx_fd = Fd(self.rx_fd.as_raw_fd());
            syscall::read(rx_fd, &mut buf[..n]).map_err(syscall_err)?;
        }

        let addr = Ipv4Addr::new(resp.addr[0], resp.addr[1], resp.addr[2], resp.addr[3]);
        Ok((n, SocketAddr::V4(SocketAddrV4::new(addr, resp.port))))
    }

    pub fn peek_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "peek not supported"))
    }

    pub fn send_to(&self, buf: &[u8], addr: &SocketAddr) -> io::Result<usize> {
        let (ip, port) = addr_to_v4(addr)?;

        // Write data to TX pipe first, then send control message
        let tx_fd = Fd(self.tx_fd.as_raw_fd());
        if !buf.is_empty() {
            syscall::write(tx_fd, buf).map_err(syscall_err)?;
        }

        let sent = toyos::net::udp_send_to(self.socket_id(), ip, port, buf.len() as u16)
            .map_err(net_err_to_io)?;
        Ok(sent as usize)
    }

    pub fn duplicate(&self) -> io::Result<UdpSocket> {
        let new_tx_fd = syscall::dup(Fd(self.tx_fd.as_raw_fd())).map_err(syscall_err)?;
        let new_rx_fd = syscall::dup(Fd(self.rx_fd.as_raw_fd())).map_err(syscall_err)?;
        Ok(UdpSocket {
            socket: Arc::clone(&self.socket),
            tx_fd: unsafe { OwnedFd::from_raw_fd(new_tx_fd.0) },
            rx_fd: unsafe { OwnedFd::from_raw_fd(new_rx_fd.0) },
            local: self.local,
            peer: crate::sync::Mutex::new(*self.peer.lock().unwrap()),
            read_timeout_ms: AtomicU32::new(self.read_timeout_ms.load(Relaxed)),
            write_timeout_ms: AtomicU32::new(self.write_timeout_ms.load(Relaxed)),
        })
    }

    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.read_timeout_ms.store(duration_to_ms(dur), Relaxed);
        Ok(())
    }

    pub fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.write_timeout_ms.store(duration_to_ms(dur), Relaxed);
        Ok(())
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        let ms = self.read_timeout_ms.load(Relaxed);
        Ok(if ms == 0 { None } else { Some(Duration::from_millis(ms as u64)) })
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        let ms = self.write_timeout_ms.load(Relaxed);
        Ok(if ms == 0 { None } else { Some(Duration::from_millis(ms as u64)) })
    }

    pub fn set_broadcast(&self, _broadcast: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn broadcast(&self) -> io::Result<bool> {
        Ok(false)
    }

    pub fn set_multicast_loop_v4(&self, _: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        Ok(false)
    }

    pub fn set_multicast_ttl_v4(&self, _: u32) -> io::Result<()> {
        Ok(())
    }

    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        Ok(1)
    }

    pub fn set_multicast_loop_v6(&self, _: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        Ok(false)
    }

    pub fn join_multicast_v4(&self, _: &Ipv4Addr, _: &Ipv4Addr) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "multicast not supported"))
    }

    pub fn join_multicast_v6(&self, _: &Ipv6Addr, _: u32) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "multicast not supported"))
    }

    pub fn leave_multicast_v4(&self, _: &Ipv4Addr, _: &Ipv4Addr) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "multicast not supported"))
    }

    pub fn leave_multicast_v6(&self, _: &Ipv6Addr, _: u32) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "multicast not supported"))
    }

    pub fn set_ttl(&self, _: u32) -> io::Result<()> {
        Ok(())
    }

    pub fn ttl(&self) -> io::Result<u32> {
        Ok(64)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }

    pub fn set_nonblocking(&self, _: bool) -> io::Result<()> {
        Ok(())
    }

    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        let (n, _) = self.recv_from(buf)?;
        Ok(n)
    }

    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "peek not supported"))
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        let peer = self.peer.lock().unwrap().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "not connected")
        })?;
        self.send_to(buf, &peer)
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses found")
        })?;
        *self.peer.lock().unwrap() = Some(addr);
        Ok(())
    }
}

// No Drop impl — Arc<NetdSocket> handles close on last drop.
// OwnedFd drops close the pipe fds.

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UdpSocket(local={})", self.local)
    }
}

// --- LookupHost (DNS) ---

pub struct LookupHost {
    addrs: Vec<SocketAddr>,
    pos: usize,
}

impl Iterator for LookupHost {
    type Item = SocketAddr;
    fn next(&mut self) -> Option<SocketAddr> {
        if self.pos < self.addrs.len() {
            let addr = self.addrs[self.pos];
            self.pos += 1;
            Some(addr)
        } else {
            None
        }
    }
}

pub fn lookup_host(host: &str, port: u16) -> io::Result<LookupHost> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(LookupHost {
            addrs: vec![SocketAddr::V4(SocketAddrV4::new(ip, port))],
            pos: 0,
        });
    }

    let mut results = [[0u8; 4]; 16];
    let count = toyos::net::dns_lookup(host, &mut results).map_err(net_err_to_io)?;

    if count == 0 {
        return Err(io::Error::new(io::ErrorKind::Other, "DNS lookup failed: no results"));
    }

    let addrs = results[..count]
        .iter()
        .map(|ip| SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::from(*ip), port)))
        .collect();

    Ok(LookupHost { addrs, pos: 0 })
}

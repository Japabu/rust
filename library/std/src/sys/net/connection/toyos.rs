use crate::cell::Cell;
use crate::fmt;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::net::{Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, SocketAddrV4, ToSocketAddrs};
use crate::os::toyos::io as toyos_io;
use crate::os::toyos::message::{self, Message};
use crate::sync::OnceLock;
use crate::time::Duration;

// --- IPC protocol constants (must match netd) ---

const MSG_TCP_CONNECT: u32 = 1;
const MSG_TCP_SEND: u32 = 2;
const MSG_TCP_RECV: u32 = 3;
const MSG_TCP_CLOSE: u32 = 4;
const MSG_TCP_BIND: u32 = 5;
const MSG_TCP_ACCEPT: u32 = 6;
const MSG_TCP_SHUTDOWN: u32 = 7;
const MSG_UDP_BIND: u32 = 8;
const MSG_UDP_SEND_TO: u32 = 9;
const MSG_UDP_RECV_FROM: u32 = 10;
const MSG_UDP_CLOSE: u32 = 11;
const MSG_DNS_LOOKUP: u32 = 12;
const MSG_TCP_SET_OPTION: u32 = 13;
const MSG_ERROR: u32 = 129;

const ERR_CONNECTION_REFUSED: u32 = 1;
const ERR_CONNECTION_RESET: u32 = 2;
const ERR_TIMED_OUT: u32 = 3;
const ERR_ADDR_IN_USE: u32 = 5;
const ERR_NOT_CONNECTED: u32 = 6;
const ERR_INVALID_INPUT: u32 = 7;

const OPT_NODELAY: u32 = 1;

// --- IPC payload structures (must match netd) ---

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpConnectRequest {
    addr: [u8; 4],
    port: u16,
    _pad: u16,
    timeout_ms: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpConnectResponse {
    socket_id: u32,
    local_port: u16,
    _pad: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpRecvRequest {
    socket_id: u32,
    max_len: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpCloseRequest {
    socket_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpBindRequest {
    addr: [u8; 4],
    port: u16,
    _pad: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpBindResponse {
    socket_id: u32,
    bound_port: u16,
    _pad: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpAcceptRequest {
    socket_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpAcceptResponse {
    socket_id: u32,
    remote_addr: [u8; 4],
    remote_port: u16,
    local_port: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TcpShutdownRequest {
    socket_id: u32,
    how: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UdpBindRequest {
    addr: [u8; 4],
    port: u16,
    _pad: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UdpBindResponse {
    socket_id: u32,
    bound_port: u16,
    _pad: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UdpRecvFromRequest {
    socket_id: u32,
    max_len: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SocketOptionRequest {
    socket_id: u32,
    option: u32,
    value: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ErrorResponse {
    code: u32,
}

// --- Helpers ---

fn netd_pid() -> u32 {
    static PID: OnceLock<u32> = OnceLock::new();
    *PID.get_or_init(|| {
        for _ in 0..100 {
            if let Some(pid) = toyos_io::find_pid("netd") {
                return pid;
            }
            toyos_io::poll_timeout(&[], 10_000_000); // 10ms
        }
        panic!("netd not found");
    })
}

fn request<T: Copy>(msg_type: u32, payload: T) -> Message {
    message::send(netd_pid(), Message::new(msg_type, payload));
    message::recv()
}

fn request_bytes(msg_type: u32, data: &[u8]) -> Message {
    message::send(netd_pid(), Message::from_bytes(msg_type, data));
    message::recv()
}

fn error_from_code(code: u32) -> io::Error {
    let kind = match code {
        ERR_CONNECTION_REFUSED => io::ErrorKind::ConnectionRefused,
        ERR_CONNECTION_RESET => io::ErrorKind::ConnectionReset,
        ERR_TIMED_OUT => io::ErrorKind::TimedOut,
        ERR_ADDR_IN_USE => io::ErrorKind::AddrInUse,
        ERR_NOT_CONNECTED => io::ErrorKind::NotConnected,
        ERR_INVALID_INPUT => io::ErrorKind::InvalidInput,
        _ => io::ErrorKind::Other,
    };
    io::Error::from(kind)
}

fn check_response(msg: &Message) -> io::Result<()> {
    if msg.msg_type() == MSG_ERROR {
        return Err(io::Error::from(io::ErrorKind::Other));
    }
    Ok(())
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

// --- TcpStream ---

pub struct TcpStream {
    socket_id: u32,
    peer: SocketAddr,
    local_port: u16,
    read_timeout_ms: Cell<u32>,
    write_timeout_ms: Cell<u32>,
    nodelay: Cell<bool>,
}

impl TcpStream {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<TcpStream> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses found")
        })?;
        Self::connect_timeout(&addr, Duration::from_secs(30))
    }

    pub fn connect_timeout(addr: &SocketAddr, timeout: Duration) -> io::Result<TcpStream> {
        let (ip, port) = addr_to_v4(addr)?;
        let resp = request(MSG_TCP_CONNECT, TcpConnectRequest {
            addr: ip,
            port,
            _pad: 0,
            timeout_ms: duration_to_ms(Some(timeout)),
        });
        if resp.msg_type() == MSG_ERROR {
            let err: ErrorResponse = resp.take_payload();
            return Err(error_from_code(err.code));
        }
        let resp: TcpConnectResponse = resp.take_payload();
        Ok(TcpStream {
            socket_id: resp.socket_id,
            peer: *addr,
            local_port: resp.local_port,
            read_timeout_ms: Cell::new(0),
            write_timeout_ms: Cell::new(0),
            nodelay: Cell::new(false),
        })
    }

    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.read_timeout_ms.set(duration_to_ms(dur));
        Ok(())
    }

    pub fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.write_timeout_ms.set(duration_to_ms(dur));
        Ok(())
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        let ms = self.read_timeout_ms.get();
        Ok(if ms == 0 { None } else { Some(Duration::from_millis(ms as u64)) })
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        let ms = self.write_timeout_ms.get();
        Ok(if ms == 0 { None } else { Some(Duration::from_millis(ms as u64)) })
    }

    pub fn peek(&self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "peek not supported"))
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let resp = request(MSG_TCP_RECV, TcpRecvRequest {
            socket_id: self.socket_id,
            max_len: buf.len() as u32,
        });
        if resp.msg_type() == MSG_ERROR {
            let err: ErrorResponse = resp.take_payload();
            return Err(error_from_code(err.code));
        }
        let data = resp.take_bytes();
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok(n)
    }

    pub fn read_buf(&self, mut buf: BorrowedCursor<'_>) -> io::Result<()> {
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
        let mut data = Vec::with_capacity(4 + buf.len());
        data.extend_from_slice(&self.socket_id.to_le_bytes());
        data.extend_from_slice(buf);
        let resp = request_bytes(MSG_TCP_SEND, &data);
        if resp.msg_type() == MSG_ERROR {
            let err: ErrorResponse = resp.take_payload();
            return Err(error_from_code(err.code));
        }
        let sent: u32 = resp.take_payload();
        Ok(sent as usize)
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
        let resp = request(MSG_TCP_SHUTDOWN, TcpShutdownRequest {
            socket_id: self.socket_id,
            how: how_val,
        });
        check_response(&resp)?;
        Ok(())
    }

    pub fn duplicate(&self) -> io::Result<TcpStream> {
        Ok(TcpStream {
            socket_id: self.socket_id,
            peer: self.peer,
            local_port: self.local_port,
            read_timeout_ms: Cell::new(self.read_timeout_ms.get()),
            write_timeout_ms: Cell::new(self.write_timeout_ms.get()),
            nodelay: Cell::new(self.nodelay.get()),
        })
    }

    pub fn set_linger(&self, _linger: Option<Duration>) -> io::Result<()> {
        Ok(()) // no-op
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        Ok(None)
    }

    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        let resp = request(MSG_TCP_SET_OPTION, SocketOptionRequest {
            socket_id: self.socket_id,
            option: OPT_NODELAY,
            value: nodelay as u32,
        });
        check_response(&resp)?;
        self.nodelay.set(nodelay);
        Ok(())
    }

    pub fn nodelay(&self) -> io::Result<bool> {
        Ok(self.nodelay.get())
    }

    pub fn set_ttl(&self, _ttl: u32) -> io::Result<()> {
        Ok(()) // no-op
    }

    pub fn ttl(&self) -> io::Result<u32> {
        Ok(64)
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }

    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        Ok(()) // TODO: implement non-blocking mode
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        let _ = request(MSG_TCP_CLOSE, TcpCloseRequest { socket_id: self.socket_id });
    }
}

impl fmt::Debug for TcpStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TcpStream(id={}, peer={})", self.socket_id, self.peer)
    }
}

// --- TcpListener ---

pub struct TcpListener {
    socket_id: u32,
    local: SocketAddr,
}

impl TcpListener {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses found")
        })?;
        let (ip, port) = addr_to_v4(&addr)?;
        let resp = request(MSG_TCP_BIND, TcpBindRequest {
            addr: ip,
            port,
            _pad: 0,
        });
        if resp.msg_type() == MSG_ERROR {
            let err: ErrorResponse = resp.take_payload();
            return Err(error_from_code(err.code));
        }
        let resp: TcpBindResponse = resp.take_payload();
        Ok(TcpListener {
            socket_id: resp.socket_id,
            local: SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from(ip),
                resp.bound_port,
            )),
        })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }

    pub fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let resp = request(MSG_TCP_ACCEPT, TcpAcceptRequest {
            socket_id: self.socket_id,
        });
        if resp.msg_type() == MSG_ERROR {
            let err: ErrorResponse = resp.take_payload();
            return Err(error_from_code(err.code));
        }
        let resp: TcpAcceptResponse = resp.take_payload();
        let peer = SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::from(resp.remote_addr),
            resp.remote_port,
        ));
        Ok((
            TcpStream {
                socket_id: resp.socket_id,
                peer,
                local_port: resp.local_port,
                read_timeout_ms: Cell::new(0),
                write_timeout_ms: Cell::new(0),
                nodelay: Cell::new(false),
            },
            peer,
        ))
    }

    pub fn duplicate(&self) -> io::Result<TcpListener> {
        Ok(TcpListener {
            socket_id: self.socket_id,
            local: self.local,
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

    pub fn set_nonblocking(&self, _nonblocking: bool) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        let _ = request(MSG_TCP_CLOSE, TcpCloseRequest { socket_id: self.socket_id });
    }
}

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TcpListener(id={}, local={})", self.socket_id, self.local)
    }
}

// --- UdpSocket ---

pub struct UdpSocket {
    socket_id: u32,
    local: SocketAddr,
    peer: Cell<Option<SocketAddr>>,
    read_timeout_ms: Cell<u32>,
    write_timeout_ms: Cell<u32>,
}

impl UdpSocket {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses found")
        })?;
        let (ip, port) = addr_to_v4(&addr)?;
        let resp = request(MSG_UDP_BIND, UdpBindRequest {
            addr: ip,
            port,
            _pad: 0,
        });
        if resp.msg_type() == MSG_ERROR {
            let err: ErrorResponse = resp.take_payload();
            return Err(error_from_code(err.code));
        }
        let resp: UdpBindResponse = resp.take_payload();
        Ok(UdpSocket {
            socket_id: resp.socket_id,
            local: SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from(ip),
                resp.bound_port,
            )),
            peer: Cell::new(None),
            read_timeout_ms: Cell::new(0),
            write_timeout_ms: Cell::new(0),
        })
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.peer.get().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "not connected")
        })
    }

    pub fn socket_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let resp = request(MSG_UDP_RECV_FROM, UdpRecvFromRequest {
            socket_id: self.socket_id,
            max_len: buf.len() as u32,
        });
        if resp.msg_type() == MSG_ERROR {
            let err: ErrorResponse = resp.take_payload();
            return Err(error_from_code(err.code));
        }
        let data = resp.take_bytes();
        if data.len() < 8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "malformed response"));
        }
        let addr = Ipv4Addr::new(data[0], data[1], data[2], data[3]);
        let port = u16::from_le_bytes([data[4], data[5]]);
        let payload = &data[8..];
        let n = payload.len().min(buf.len());
        buf[..n].copy_from_slice(&payload[..n]);
        Ok((n, SocketAddr::V4(SocketAddrV4::new(addr, port))))
    }

    pub fn peek_from(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "peek not supported"))
    }

    pub fn send_to(&self, buf: &[u8], addr: &SocketAddr) -> io::Result<usize> {
        let (ip, port) = addr_to_v4(addr)?;
        // Format: [socket_id:4][addr:4][port:2][pad:2][data...]
        let mut data = Vec::with_capacity(12 + buf.len());
        data.extend_from_slice(&self.socket_id.to_le_bytes());
        data.extend_from_slice(&ip);
        data.extend_from_slice(&port.to_le_bytes());
        data.extend_from_slice(&[0, 0]);
        data.extend_from_slice(buf);
        let resp = request_bytes(MSG_UDP_SEND_TO, &data);
        if resp.msg_type() == MSG_ERROR {
            let err: ErrorResponse = resp.take_payload();
            return Err(error_from_code(err.code));
        }
        let sent: u32 = resp.take_payload();
        Ok(sent as usize)
    }

    pub fn duplicate(&self) -> io::Result<UdpSocket> {
        Ok(UdpSocket {
            socket_id: self.socket_id,
            local: self.local,
            peer: Cell::new(self.peer.get()),
            read_timeout_ms: Cell::new(self.read_timeout_ms.get()),
            write_timeout_ms: Cell::new(self.write_timeout_ms.get()),
        })
    }

    pub fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.read_timeout_ms.set(duration_to_ms(dur));
        Ok(())
    }

    pub fn set_write_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        self.write_timeout_ms.set(duration_to_ms(dur));
        Ok(())
    }

    pub fn read_timeout(&self) -> io::Result<Option<Duration>> {
        let ms = self.read_timeout_ms.get();
        Ok(if ms == 0 { None } else { Some(Duration::from_millis(ms as u64)) })
    }

    pub fn write_timeout(&self) -> io::Result<Option<Duration>> {
        let ms = self.write_timeout_ms.get();
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
        let peer = self.peer.get().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotConnected, "not connected")
        })?;
        self.send_to(buf, &peer)
    }

    pub fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        let addr = addr.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "no addresses found")
        })?;
        self.peer.set(Some(addr));
        Ok(())
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        let _ = request(MSG_UDP_CLOSE, TcpCloseRequest { socket_id: self.socket_id });
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UdpSocket(id={}, local={})", self.socket_id, self.local)
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
    // Try parsing as IP literal first
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Ok(LookupHost {
            addrs: vec![SocketAddr::V4(SocketAddrV4::new(ip, port))],
            pos: 0,
        });
    }

    // Send DNS query to netd
    let resp = request_bytes(MSG_DNS_LOOKUP, host.as_bytes());
    if resp.msg_type() == MSG_ERROR {
        return Err(io::Error::new(io::ErrorKind::Other, "DNS lookup failed"));
    }

    let data = resp.take_bytes();
    if data.is_empty() {
        return Err(io::Error::new(io::ErrorKind::Other, "DNS lookup failed: empty response"));
    }

    let count = data[0] as usize;
    let mut addrs = Vec::with_capacity(count);
    let mut offset = 1;
    for _ in 0..count {
        if offset >= data.len() {
            break;
        }
        match data[offset] {
            4 if offset + 5 <= data.len() => {
                let ip = Ipv4Addr::new(
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                    data[offset + 4],
                );
                addrs.push(SocketAddr::V4(SocketAddrV4::new(ip, port)));
                offset += 5;
            }
            _ => break,
        }
    }

    if addrs.is_empty() {
        return Err(io::Error::new(io::ErrorKind::Other, "DNS lookup failed: no results"));
    }

    Ok(LookupHost { addrs, pos: 0 })
}

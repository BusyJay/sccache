use std::fmt;

use futures::{Future, TryFutureExt};
use tokio::{io::{AsyncRead, AsyncWrite}, net};

#[derive(Debug)]
pub enum SocketAddr {
    Net(std::net::SocketAddr),
    Unix(std::path::PathBuf),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    UnixAbstract(Vec<u8>),
}

impl fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocketAddr::Net(addr) => write!(f, "{}", addr),
            SocketAddr::Unix(p) => write!(f, "{}", p.display()),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            SocketAddr::UnixAbstract(p) => write!(f, "{}", p.escape_ascii()),
        }
    }
}

impl SocketAddr {
    /// Parse a string into a `SocketAddr`.
    ///
    /// The string should follow the format of `self.to_string()`.
    pub fn parse(s: &str) -> Self {
        // Parse abstract socket address first as it can contain any chars.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            if s.starts_with('\x00') {
                let data = crate::util::ascii_unescape_default(s.as_bytes());
                return SocketAddr::UnixAbstract(data);
            }
        }
        // Usually a colon won't appears in unix path.
        if s.contains(':') {
            if let Ok(addr) = s.parse() {
                return SocketAddr::Net(addr);
            }
        }
        // Windows path may contain ':'.
        let path = std::path::PathBuf::from(s);
        SocketAddr::Unix(path)
    }
}

pub trait Acceptor {
    type Socket: AsyncRead + AsyncWrite + Unpin + Send;

    fn accept(&self) -> impl Future<Output=tokio::io::Result<Self::Socket>> + Send;
    fn local_addr(&self) -> tokio::io::Result<SocketAddr>;
}

impl Acceptor for net::TcpListener {
    type Socket = net::TcpStream;

    #[inline]
    fn accept(&self) -> impl Future<Output=tokio::io::Result<Self::Socket>> + Send {
        net::TcpListener::accept(self).and_then(|(s, _)| futures::future::ok(s))
    }

    #[inline]
    fn local_addr(&self) -> tokio::io::Result<SocketAddr> {
        net::TcpListener::local_addr(&self).map(SocketAddr::Net)
    }
}

pub trait Connection: std::io::Read + std::io::Write {
    fn try_clone(&self) -> std::io::Result<Box<dyn Connection>>;
}

impl Connection for std::net::TcpStream {
    #[inline]
    fn try_clone(&self) -> std::io::Result<Box<dyn Connection>> {
        let stream = std::net::TcpStream::try_clone(self)?;
        Ok(Box::new(stream))
    }
}

pub fn connect(addr: &SocketAddr) -> std::io::Result<Box<dyn Connection>> {
    match addr {
        SocketAddr::Net(addr) => std::net::TcpStream::connect(addr).map(|s| Box::new(s) as Box<dyn Connection>),
        #[cfg(unix)]
        SocketAddr::Unix(p) => std::os::unix::net::UnixStream::connect(p).map(|s| Box::new(s) as Box<dyn Connection>),
        #[cfg(any(target_os = "linux", target_os = "android"))]
        SocketAddr::UnixAbstract(p) => {
            let sock = std::os::unix::net::SocketAddr::from_abstract_name(p);
            std::os::unix::net::UnixStream::connect_addr(sock).map(|s| Box::new(s) as Box<dyn Connection>)
        }
    }
}

#[cfg(unix)]
mod unix_imp {
    use std::path::PathBuf;

    use futures::TryFutureExt;

    use super::*;

    impl Acceptor for net::UnixListener {
        type Socket = net::UnixStream;

        #[inline]
        fn accept(&self) -> impl Future<Output=tokio::io::Result<Self::Socket>> + Send {
            net::UnixListener::accept(self).and_then(|(s, _)| futures::future::ok(s))
        }

        #[inline]
        fn local_addr(&self) -> tokio::io::Result<SocketAddr> {
            let addr = net::UnixListener::local_addr(self)?;
            if let Some(p) = addr.as_pathname() {
                return Ok(SocketAddr::Unix(p.to_path_buf()));
            }
            #[cfg(any(target_os = "linux", target_os = "android"))]
            if let Some(p) = addr.as_abstract_name() {
                return Ok(SocketAddr::UnixAbstract(p.to_vec()));
            }
            Ok(SocketAddr::Unix(PathBuf::new()))
        }
    }

    impl Connection for std::os::unix::net::UnixStream {
        #[inline]
        fn try_clone(&self) -> std::io::Result<Box<dyn Connection>> {
            let stream = std::os::unix::net::UnixStream::try_clone(self)?;
            Ok(Box::new(stream))
        }
    }
}
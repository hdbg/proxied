use std::{collections::HashMap, net::SocketAddr, str::FromStr, sync::LazyLock};

use async_http_proxy::HttpError;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    sync::Mutex,
};

use crate::{Proxy, ProxyKind};

// TODO: refactor this to provide more details
#[derive(thiserror::Error, Debug)]
pub enum ConnectError {
    #[error("No DNS records were present for this domain")]
    DnsNameNotResolved,

    #[error("Input/Output fail")]
    IO(#[from] std::io::Error),

    #[error("HTTP tunnel failed to connect")]
    Http(#[from] HttpError),

    #[error("SOCKS tunnel failed to connect")]
    Socks(#[from] fast_socks5::SocksError),

    #[error("Authentication Failed")]
    AuthFailed { details: Option<String> },

    #[error("Authentication method is unacceptable")]
    AuthMethodUnacceptable,

    #[error("Failed proxy.addr parsing")]
    FailedAddrParsing,

    #[error("Wrong protocol used")]
    WrongProtocol,

    #[error("Passed connection domain is too long")]
    ExceededMaxDomainLen,
}

#[derive(Debug)]
/// Target for proxy for connection, in form of DNS name or socket's IP Address
///
/// Each Domain target is cached, and if you make multiple connections
/// to a single domain, where multiple A records exists
/// will perform a round-robin to distribute load
///
/// > **Note**: There is a limit on cached entries, so your memory won't run out
pub enum NetworkTarget {
    Domain { domain: String, port: u16 },
    IPAddr { socket: SocketAddr },
}

impl NetworkTarget {
    pub fn host(&self) -> String {
        match &self {
            NetworkTarget::Domain { domain, .. } => domain.clone(),
            NetworkTarget::IPAddr { socket } => socket.ip().to_string(),
        }
    }

    pub fn port(&self) -> u16 {
        match &self {
            NetworkTarget::Domain { port, .. } => *port,
            NetworkTarget::IPAddr { socket } => socket.port(),
        }
    }
}

impl std::fmt::Display for NetworkTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            NetworkTarget::Domain { domain, port } => {
                f.write_fmt(format_args!("{}:{}", domain, port))
            }
            NetworkTarget::IPAddr { socket } => f.write_str(&socket.to_string()),
        }
    }
}
trait BiConnection: AsyncRead + AsyncWrite + Unpin {}

impl<T: AsyncRead + AsyncWrite + Unpin> BiConnection for T {}
trait ProxyProto {
    async fn new(
        proxy: &Proxy,
        target: NetworkTarget,
        proxy_stream: tokio::net::TcpStream,
    ) -> Result<Box<dyn BiConnection>, ConnectError>;
}

mod socks_proto {
    use fast_socks5::{client::Config, util::target_addr::TargetAddr, AuthenticationMethod};
    use tokio::net::TcpStream;

    use crate::Proxy;

    use super::{BiConnection, ConnectError, NetworkTarget, ProxyProto};

    impl From<NetworkTarget> for TargetAddr {
        fn from(val: NetworkTarget) -> Self {
            match val {
                NetworkTarget::Domain { domain, port } => TargetAddr::Domain(domain, port),
                NetworkTarget::IPAddr { socket } => TargetAddr::Ip(socket),
            }
        }
    }

    pub struct SocksProtocol;
    impl ProxyProto for SocksProtocol {
        async fn new(
            proxy: &Proxy,
            target: NetworkTarget,
            proxy_stream: TcpStream,
        ) -> Result<Box<dyn BiConnection>, ConnectError> {
            let mut auth = None;
            if let Some((username, password)) = &proxy.creds {
                auth = Some(AuthenticationMethod::Password {
                    username: username.clone(),
                    password: password.clone(),
                });
            }
            let stream = fast_socks5::client::Socks5Stream::use_stream(
                proxy_stream,
                auth,
                Config::default(),
            )
            .await;

            let mut stream = match stream {
                Ok(stream) => stream,
                Err(error) => match error {
                    fast_socks5::SocksError::AuthMethodUnacceptable(_) => {
                        return Err(ConnectError::AuthMethodUnacceptable);
                    }
                    fast_socks5::SocksError::UnsupportedSocksVersion(_) => {
                        return Err(ConnectError::WrongProtocol);
                    }
                    fast_socks5::SocksError::AuthenticationFailed(details) => {
                        return Err(ConnectError::AuthFailed {
                            details: Some(details),
                        });
                    }
                    fast_socks5::SocksError::AuthenticationRejected(details) => {
                        return Err(ConnectError::AuthFailed {
                            details: Some(details),
                        });
                    }

                    err => return Err(err.into()),
                },
            };

            let command_result = stream
                .request(fast_socks5::Socks5Command::TCPConnect, target.into())
                .await;

            match command_result {
                Ok(_) => Ok(Box::new(stream)),
                Err(fast_socks5::SocksError::ExceededMaxDomainLen(_)) => {
                    Err(ConnectError::ExceededMaxDomainLen)
                }
                Err(e) => Err(e.into()),
            }
        }
    }
}

mod http_proto {
    use async_http_proxy::HttpError;
    use tokio::net::TcpStream;

    use crate::Proxy;

    use super::{BiConnection, ConnectError, NetworkTarget, ProxyProto};

    pub struct HttpProtocol;
    impl ProxyProto for HttpProtocol {
        async fn new(
            proxy: &Proxy,
            target: NetworkTarget,
            mut proxy_stream: TcpStream,
        ) -> Result<Box<dyn BiConnection>, ConnectError> {
            let host = target.host();
            let resp = match &proxy.creds {
                Some((login, password)) => {
                    async_http_proxy::http_connect_tokio_with_basic_auth(
                        &mut proxy_stream,
                        host.as_str(),
                        target.port(),
                        login.as_str(),
                        password.as_str(),
                    )
                    .await
                }
                None => {
                    async_http_proxy::http_connect_tokio(
                        &mut proxy_stream,
                        host.as_str(),
                        target.port(),
                    )
                    .await
                }
            };

            match resp {
                Ok(()) => (),
                Err(HttpError::IoError(io)) => return Err(ConnectError::IO(io)),
                Err(HttpError::HttpCode200(407)) => {
                    return Err(ConnectError::AuthFailed { details: None })
                }

                Err(err) => return Err(err.into()),
            }

            Ok(Box::new(proxy_stream))
        }
    }
}

/// TCP Tunnel through proxy server
///
/// Create using [`Proxy::connect`]
/// Internally uses protocol of proxy server to connect

pub struct TCPConnection(Box<dyn BiConnection>);
impl AsyncRead for TCPConnection {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let pinned = std::pin::pin!(&mut self.0);
        pinned.poll_read(cx, buf)
    }
}

impl AsyncWrite for TCPConnection {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let pinned = std::pin::pin!(&mut self.0);
        pinned.poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let pinned = std::pin::pin!(&mut self.0);
        pinned.poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let pinned = std::pin::pin!(&mut self.0);
        pinned.poll_flush(cx)
    }
}

pub struct AddrRecord {
    items: Vec<SocketAddr>,
    next_item: usize,
}

const CACHE_SIZE: usize = 1_000;
const CACHE_THRESHOLD: usize = CACHE_SIZE + CACHE_SIZE / 2;

/// Cached names to perform round-robin in case that there are multiple connections to same domain
static RESOLVED_DNS: LazyLock<Mutex<HashMap<String, AddrRecord>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

async fn name_present_dns(record: &mut AddrRecord) -> Result<SocketAddr, ConnectError> {
    if record.items.is_empty() {
        Err(ConnectError::DnsNameNotResolved)
    } else {
        let current = record
            .items
            .get(record.next_item)
            .ok_or(ConnectError::DnsNameNotResolved)?;
        record.next_item += 1;

        if record.next_item == record.items.len() {
            record.next_item += 1;
        }

        Ok(*current)
    }
}
async fn resolve_dns(domain: &String) -> Result<SocketAddr, ConnectError> {
    let mut records_lock = RESOLVED_DNS.lock().await;

    // safety precaution not to fill all the heap with cache (very unlikely, but should be handle)
    if records_lock.len() > CACHE_THRESHOLD {
        let mut size_delta = records_lock.len() - CACHE_SIZE;
        records_lock.retain(|_, _| {
            if size_delta > 0 {
                size_delta -= 1;
                return false;
            }
            true
        });
    }

    if let Some(record) = records_lock.get_mut(domain) {
        name_present_dns(record).await
    } else {
        // free lock while resolving process takes places in order to give change other threads to lock  while we resolve and to avoid deadlock by reccurent locking
        drop(records_lock);

        let domain_name = format!("{}:1", &domain);
        let resolve_request = tokio::net::lookup_host(domain_name).await?.collect();

        // kickstart lock
        records_lock = RESOLVED_DNS.lock().await;

        // check if it wasn't resolved by another thread in mean time
        //
        // it's needed because we can accidentally overwrite round robin state
        // meaning that may be other threads already used `next_time` and updated it.
        // although not critical, we don't want to lose this information

        if !records_lock.contains_key(domain) {
            records_lock.insert(
                domain.clone(),
                AddrRecord {
                    items: resolve_request,
                    next_item: 0,
                },
            );
        }

        name_present_dns(records_lock.get_mut(domain).unwrap()).await
    }
}

pub async fn connect(proxy: &Proxy, target: NetworkTarget) -> Result<TCPConnection, ConnectError> {
    let resolved_addr = match proxy.is_dns_addr() {
        true => resolve_dns(&proxy.addr).await?,
        false => SocketAddr::from_str(&format!("{}:{}", &proxy.addr, proxy.port))
            .map_err(|_| ConnectError::FailedAddrParsing)?,
    };

    let stream = TcpStream::connect(resolved_addr).await?;
    let conn = match &proxy.kind {
        ProxyKind::Socks5 | ProxyKind::Socks4 => {
            socks_proto::SocksProtocol::new(proxy, target, stream).await?
        }
        ProxyKind::Http | ProxyKind::Https => {
            http_proto::HttpProtocol::new(proxy, target, stream).await?
        }
    };

    Ok(TCPConnection(conn))
}

#[cfg(test)]
mod tests {}

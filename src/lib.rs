/*!
# `Proxied`
Asynchronous proxy TCP connector

Includes:
- No `unsafe` code
- SOCKS4/5 and HTTP(s) proxies support
- Single structure for both types of proxies
- [`TCPStream`](tokio::net::TcpStream)-like connection (see [`TCPConnection`])
- Password authentication

## How-to
Main entrypoint is [`Proxy`] structure.
It contains connection data about proxy like protocol, address port and credentials.
Additionally it supports IP refreshment link, although user is expected to manually request it.

To create a TCP connection, call [`Proxy::connect_tcp`]. After it is created, it can be used
just like regural TCP stream, as it implements [`AsyncRead`](tokio::io::AsyncRead) and [`AsyncWrite`](tokio::io::AsyncRead).
*/

/** Proxy protocol

Backend protocol of proxy server. Doesn't affect developer experience, except:
- SOCKS4/5 proxies are fully and always supported
- HTTP(s) proxy servers are expected to implement `CONNECT` method (see [RFC7232](https://datatracker.ietf.org/doc/html/rfc7231#section-4.3.6))
*/
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProxyKind {
    Socks5,
    Socks4,
    Http,
    Https,
}

/**
Proxy connection data

Support mobile proxies by including `refresh_link`, although `connect` method won't
automatically refresh proxy on each connect

 ## [`FromStr`] Format

 `<protocol>://(login:password)@ip:port`

 Credentials are optional.

 Example:
```rust
use std::str::FromStr;

let socks = "socks4://hello:world@127.0.0.1:1234";
let proxy = proxied::Proxy::from_str(socks).unwrap();
assert_eq!(&proxy.addr, "127.0.0.1");
```
*/
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Proxy {
    pub kind: ProxyKind,
    pub addr: String,
    pub port: u16,
    pub creds: Option<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_url: Option<String>,
}

impl Proxy {
    pub fn is_dns_addr(&self) -> bool {
        self.addr.chars().any(char::is_alphabetic)
    }

    pub fn is_ip_addr(&self) -> bool {
        !self.is_dns_addr()
    }

    /// Create TCP tunnel through this proxy to the target
    pub async fn connect_tcp(&self, target: NetworkTarget) -> Result<TcpStream, ConnectError> {
        connect::connect(self, target).await
    }
}
pub mod parse;

mod connect;

pub use connect::{ConnectError, NetworkTarget};
use tokio::net::TcpStream;

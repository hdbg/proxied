#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum ProxyKind {
    Socks5,
    Socks4,
    Http,
    Https,
}

/**
 Unified proxy structure with support of `refresh_url`, which is a link to request IP address refresh of proxy

 ## [FromStr] Format

 <protocol>://(login:password)@ip:port

 Example:
```
use std::str::FromStr;

let socks = "socks4://hello:world@127.0.0.1:1234";
let proxy = proxied::Proxy::from_str(socks).unwrap();
assert_eq!(&proxy.addr, "127.0.0.1");
```
*/

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Proxy {
    pub kind: ProxyKind,
    pub addr: String,
    pub port: u16,
    pub creds: Option<(String, String)>,
    pub refresh_url: Option<String>,
}

impl Proxy {
    pub fn is_dns_addr(&self) -> bool {
        self.addr.chars().any(char::is_alphabetic)
    }

    pub fn is_ip_addr(&self) -> bool {
        !self.is_dns_addr()
    }

    pub async fn connect(
        &self,
        target: NetworkTarget,
    ) -> Result<connect::ProxyConnection, ConnectError> {
        connect::connect(&self, target).await
    }
}
pub mod parse;

mod connect;

pub use connect::{ConnectError, NetworkTarget, ProxyConnection};

# Proxied
Async SOCKS4/5 and HTTP(s) client connectors for Rust

## Features
- HTTP/HTTPS proxy
- SOCKS4/5 proxy
- Basic authorization for each type of proxy
- Fully **async**
- Proxy address as DNS Name
- Round-robin dispatch in case of multiple addresses

## Getting started
Add the following to your `Cargo.toml` file:
```toml
[dependencies]
proxied = "0.3"
```

## Example
```rust
use proxied::{Proxy, TCPConnection, NetworkTarget};

#[tokio::main]
async fn main() {
    let proxy = Proxy::from_str("socks5://127.0.0.1:1080").unwrap();
    let connection = proxy.connect(NetworkTarget::Domain {domain: "tcpbin.com", port: 4242}).await.unwrap();

    // Send data to the echo server
    let data = &[1, 2, 3, 4, 5, 6, 7, 8, 9];
    connection.write(data).await.unwrap(); 

    // Read the data back
    let mut buf = vec![0; data.len()];
    connection.read_exact(&mut buf).await.unwrap();

}
```
    
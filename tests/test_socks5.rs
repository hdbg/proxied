use std::{convert::Infallible, ops::Deref, sync::LazyLock};

use anyhow::bail;
use gerevs::{
    auth::username_password_authenticator::{
        User, UserAuthenticator, UsernamePasswordAuthenticator,
    },
    method_handlers, Socks5Socket,
};
use proxied::Proxy;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    task::JoinSet,
};

const SOCKS_SERVER_LISTENER_PORT: u16 = 1034;

static PROXY_USER: LazyLock<User> = LazyLock::new(|| User {
    username: "proxied".to_string(),
    password: "proxied1234".to_string(),
});

pub struct SimpleUserAuthenticator {}
impl UserAuthenticator for SimpleUserAuthenticator {
    type Credentials = ();

    fn authenticate_user(
        &mut self,
        user: User,
    ) -> impl std::future::Future<Output = std::io::Result<Option<Self::Credentials>>> + Send {
        async move {
            if user.password == PROXY_USER.password && user.username == PROXY_USER.username {
                Ok(Some(()))
            } else {
                Ok(None)
            }
        }
    }
}
fn proxy_user_authorizer() -> UsernamePasswordAuthenticator<SimpleUserAuthenticator> {
    UsernamePasswordAuthenticator::new(SimpleUserAuthenticator {})
}
async fn run_socks5_server_authorized() -> anyhow::Result<Infallible> {
    let mut listener =
        tokio::net::TcpListener::bind(format!("127.0.0.1:{}", SOCKS_SERVER_LISTENER_PORT)).await?;

    let mut join_set = JoinSet::new();
    loop {
        let (new_connection, _) = listener.accept().await?;
        tracing::info!(event = "server.received_conn");
        let socket = Socks5Socket::new(
            new_connection,
            proxy_user_authorizer(),
            method_handlers::TunnelConnect,
            method_handlers::BindDenier,
            method_handlers::AssociateDenier,
        );
        join_set.spawn(async move { socket.run().await });
        tracing::info!(event = "server.initialized_socks");
    }
}

#[tokio::test]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let _server = tokio::task::spawn(run_socks5_server_authorized());

    let proxy = Proxy {
        kind: proxied::ProxyKind::Socks5,
        addr: "127.0.0.1".to_string(),
        port: SOCKS_SERVER_LISTENER_PORT,
        creds: Some((PROXY_USER.username.clone(), PROXY_USER.password.clone())),
        refresh_url: None,
    };

    let mut connection = proxy
        .connect(proxied::NetworkTarget::Domain {
            domain: "tcpbin.com".to_string(),
            port: 4242,
        })
        .await
        .unwrap();
    tracing::info!(event = "client.initialized_socks");

    // send one byte to echo_bin

    if connection.write(&[1]).await? == 0 {
        tracing::error!(event = "client.unexpected_close");
        bail!("connection closed");
    }

    let mut recv_buffer: Vec<u8> = Vec::with_capacity(1);

    if connection.read(recv_buffer.as_mut_slice()).await? == 0 {
        tracing::error!(event = "client.unexpected_close", action = "read");
        bail!("connection closed");
    }

    Ok(())
}

use anyhow::{anyhow, bail};
use fast_socks5::{
    server::{Config, SimpleUserPassword, Socks5Server, Socks5Socket},
    Result, SocksError,
};
use futures::{Future, StreamExt};
use proxied::Proxy;
use std::{convert::Infallible, ops::Deref, sync::LazyLock};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    task::JoinSet,
};

const SOCKS_SERVER_LISTENER_PORT: u16 = 1034;
#[derive(Debug)]
struct Opt {
    /// Bind on address address. eg. `127.0.0.1:1080`
    pub listen_addr: String,

    /// Request timeout
    pub request_timeout: u64,

    /// Choose authentication type
    pub auth: AuthMode,

    /// Don't perform the auth handshake, send directly the command request
    pub skip_auth: bool,
}

/// Choose the authentication type
#[derive(Debug)]
enum AuthMode {
    NoAuth,
    Password { username: String, password: String },
}

pub struct User {
    username: String,
    password: String,
}

static PROXY_USER: LazyLock<User> = LazyLock::new(|| User {
    username: "proxied".to_string(),
    password: "proxied1234".to_string(),
});
fn spawn_and_log_error<F, T>(fut: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = Result<Socks5Socket<T, SimpleUserPassword>>> + Send + 'static,
    T: AsyncRead + AsyncWrite + Unpin,
{
    tokio::task::spawn(async move {
        match fut.await {
            Ok(mut socket) => {
                if let Some(user) = socket.take_credentials() {
                    tracing::info!("user logged in with `{}`", user.username);
                }
            }
            Err(err) => tracing::error!("{:#}", &err),
        }
    })
}
async fn run_socks5_server(opt: Opt) -> anyhow::Result<Infallible> {
    let mut config = Config::default();
    config.set_request_timeout(opt.request_timeout);
    config.set_skip_auth(opt.skip_auth);

    let config = match opt.auth {
        AuthMode::NoAuth => {
            tracing::warn!("No authentication has been set!");
            config
        }
        AuthMode::Password { username, password } => {
            if opt.skip_auth {
                return Err(SocksError::ArgumentInputError(
                    "Can't use skip-auth flag and authentication altogether.",
                )
                .into());
            }

            tracing::info!("Simple auth system has been set.");
            config.with_authentication(SimpleUserPassword { username, password })
        }
    };

    let listener = <Socks5Server>::bind(&opt.listen_addr).await?;
    let listener = listener.with_config(config);

    let mut incoming = listener.incoming();

    tracing::info!("Listen for socks connections @ {}", &opt.listen_addr);

    // Standard TCP loop
    while let Some(socket_res) = incoming.next().await {
        match socket_res {
            Ok(socket) => {
                spawn_and_log_error(socket.upgrade_to_socks5());
            }
            Err(err) => {
                tracing::error!("accept error = {:?}", err);
                return Err(err.into());
            }
        }
    }

    Err(anyhow!("failed"))
}

#[tokio::test]
async fn test_socks5_password() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let opt = Opt {
        listen_addr: format!("127.0.0.1:{}", SOCKS_SERVER_LISTENER_PORT),
        request_timeout: 1_000,
        auth: AuthMode::Password {
            username: PROXY_USER.username.clone(),
            password: PROXY_USER.password.clone(),
        },
        skip_auth: false,
    };

    let _server = tokio::task::spawn(run_socks5_server(opt));

    let proxy = Proxy {
        kind: proxied::ProxyKind::Socks5,
        addr: "127.0.0.1".to_string(),
        port: SOCKS_SERVER_LISTENER_PORT,
        creds: Some((PROXY_USER.username.clone(), PROXY_USER.password.clone())),
        refresh_url: None,
    };

    let mut connection = proxy
        .connect_tcp(proxied::NetworkTarget::Domain {
            domain: "tcpbin.com".to_string(),
            port: 4242,
        })
        .await
        .unwrap();
    tracing::info!(event = "client.initialized_socks");

    // send one byte to echo_bin

    let VERIFICATION_SLICE: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    if connection.write(VERIFICATION_SLICE).await? == 0 {
        tracing::error!(event = "client.unexpected_close");
        bail!("connection closed");
    }

    let mut recv_buffer: Vec<u8> = vec![0; VERIFICATION_SLICE.len()];

    if connection.read(recv_buffer.as_mut_slice()).await? == 0 {}

    if recv_buffer.as_slice() == VERIFICATION_SLICE {
        tracing::info!(event = "client.slice.ok");
    } else {
        tracing::error!(event = "client.unexpected_close", action = "read");
        bail!("connection closed");
    }

    Ok(())
}

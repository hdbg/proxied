use std::str::FromStr;

use crate::{Proxy, ProxyKind};

impl FromStr for ProxyKind {
    type Err = ParseError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_ascii_lowercase().as_str() {
            "socks5" => Ok(Self::Socks5),
            "socks4" => Ok(Self::Socks4),
            "http" => Ok(Self::Http),
            "https" => Ok(Self::Https),

            _ => Err(ParseError::InvalidProxyKind),
        }
    }
}

impl std::fmt::Display for ProxyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_str(match self {
            Self::Socks4 => "socks4",
            Self::Socks5 => "socks5",
            Self::Http => "http",
            Self::Https => "https",
        })
    }
}

impl std::fmt::Display for Proxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match &self.creds {
            Some((login, password)) => f.write_fmt(format_args!(
                "{}://{}:{}@{}:{}",
                self.kind, login, password, self.addr, self.port,
            ))?,
            None => f.write_fmt(format_args!("{}://{}:{}", self.kind, self.addr, self.port))?,
        };
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Verify correctness on proxy parts")]
    InvalidChunkCount,

    #[error("Failed to parse port")]
    InvalidPort,

    #[error("Failed to recognize proxy kind")]
    InvalidProxyKind,

    #[error("Failed to parse `refresh_url`")]
    InvalidRefresh,
}
impl FromStr for Proxy {
    type Err = ParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut input = input;
        let refresh_url = if input.ends_with(']') {
            let begginning_index = input.rfind('[').ok_or(ParseError::InvalidRefresh)?;
            let (rest, refresh) = input.split_at(begginning_index);
            input = rest;
            Some(refresh.trim_start_matches('[').to_owned())
        } else {
            None
        };

        let mut input_stack = input.split(&[':', '@']).collect::<Vec<_>>();

        if !([3, 4, 5, 6].contains(&input_stack.len())) {
            return Err(ParseError::InvalidChunkCount);
        }

        let kind = ProxyKind::from_str(input_stack.remove(0))?;

        let creds = {
            if !input.contains('@') {
                None
            } else {
                // ad-hoc socks:// <- // part removing fix
                let login = input_stack.remove(0).replace("//", "");
                let password = input_stack.remove(0);

                Some((login.to_owned(), password.to_owned()))
            }
        };
        let addr = input_stack.remove(0).to_string();
        let port: u16 = input_stack
            .remove(0)
            .parse()
            .map_err(|_| ParseError::InvalidPort)?;

        Ok(Self {
            kind,
            addr,
            port,
            creds,
            refresh_url,
        })
    }
}

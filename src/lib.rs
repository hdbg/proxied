use std::str::FromStr;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum ProxyKind {
    Socks5,
    Socks4,
    Http,
    Https,
}
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
/**
 Unified proxy structure with support of `refresh_url`, which is a link to request IP address refresh of proxy

 ## [FromStr] Formats

 kind:addr:port <br />
 kind:addr:port[refresh_url] <br />
 kind:addr:port:login:password <br />
 kind:addr:port:login:password[refresh_url] <br />

 Example:
```
use std::str::FromStr;

let socks = "socks4:127.0.0.1:1234:hello:world";
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

        let mut input_stack = input.split(&[':', ';', '@']).collect::<Vec<_>>();

        if !([3, 4, 5, 6].contains(&input_stack.len())) {
            return Err(ParseError::InvalidChunkCount);
        }

        let kind = ProxyKind::from_str(&input_stack.remove(0))?;

        let creds = {
            if !input.contains('@') {
                None
            } else {
                let login = input_stack.remove(0);
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::Proxy;

    const KINDS: &'static [&'static str] = &["socks4", "socks5", "http", "https"];

    // #[test]
    // pub fn test_from() {
    //     for kind in KINDS.iter() {
    //         for refresh_url in [None, Some(String::from("[https://example.com]"))] {
    //             for credentials in [None, Some(("hello".to_owned(), "world".to_owned()))] {
    //                 for separator in [":", ";"] {
    //                     let s = separator;
    //                     let credentials = match &credentials {
    //                         Some(credentials) => {
    //                             s.to_owned() + &credentials.0 + s.into() + &credentials.1
    //                         }
    //                         None => "".to_owned(),
    //                     };
    //                     let refresh = refresh_url.clone().unwrap_or("".to_owned());
    //                     let input = ((*kind).to_owned()
    //                         + s
    //                         + "192.1.1.0"
    //                         + s
    //                         + "1234"
    //                         + &credentials
    //                         + &refresh)
    //                         .to_owned();
    //                     println!("Running: {input}");
    //                     let _: Proxy = Proxy::from_str(&input).unwrap();
    //                 }
    //             }
    //         }
    //     }
    // }

    // #[test]
    // pub fn test_from_2() {
    //     let proxy = Proxy::from_str(input).unwrap();
    // }
}

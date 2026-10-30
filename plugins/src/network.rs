use harness3::{CancellationToken, Error, Result};
use reqwest::{Client, Response, Url};
use std::{net::IpAddr, time::Duration};
pub const MAX_BODY: usize = 1024 * 1024;
pub fn client() -> Result<reqwest::ClientBuilder> {
    Ok(Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .user_agent("Harness3/0.1"))
}
pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, _, _] = v.octets();
            !v.is_private()
                && !v.is_loopback()
                && !v.is_link_local()
                && !v.is_multicast()
                && a != 0
                && a < 240
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0)
                && !(a == 198 && (b == 18 || b == 19))
                && !v.is_documentation()
        }
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                return public_ip(v4.into());
            }
            let s = v.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && s[1] < 0x200)
                && !(s[0] == 0x2001 && s[1] == 0xdb8)
                && s[0] != 0x2002
        }
    }
}
pub async fn public_client(url: &Url, cancel: &CancellationToken) -> Result<Client> {
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::new("web_url_not_allowed"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| Error::new("web_url_not_allowed"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| Error::new("web_url_not_allowed"))?;
    if port != 80 && port != 443 {
        return Err(Error::new("web_port_not_allowed"));
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        if !public_ip(ip) {
            return Err(Error::new("web_private_address"));
        }
        return client()?
            .build()
            .map_err(|_| Error::new("web_client_failed"));
    }
    let addresses = tokio::select! {
        _=cancel.cancelled()=>return Err(Error::new("cancelled")),
        result=tokio::time::timeout(Duration::from_secs(5),tokio::net::lookup_host((host,port)))=>result.map_err(|_|Error::new("web_dns_timeout"))?.map_err(|_|Error::new("web_dns_failed"))?.collect::<Vec<_>>(),
    };
    if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
        return Err(Error::new("web_private_address"));
    }
    // Pin the checked addresses so the connection cannot perform a second DNS lookup.
    client()?
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(|_| Error::new("web_client_failed"))
}
pub async fn body(mut response: Response, cancel: &CancellationToken) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(Error::new("web_http_error"));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        return Err(Error::new("web_response_too_large"));
    }
    let mut bytes = Vec::new();
    loop {
        let chunk = tokio::select! {_=cancel.cancelled()=>return Err(Error::new("cancelled")), chunk=response.chunk()=>chunk.map_err(|_|Error::new("web_read_failed"))?};
        let Some(chunk) = chunk else { break };
        if bytes.len() + chunk.len() > MAX_BODY {
            return Err(Error::new("web_response_too_large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_local_reserved_and_mapped_addresses() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "192.0.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
            "2002:7f00:1::",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(public_ip(ip.parse().unwrap()), "{ip}");
        }
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use axum::{Router, response::Redirect, routing::get};
    #[tokio::test]
    async fn rejects_redirects_and_oversized_bodies_and_honors_cancellation() {
        let app = Router::new()
            .route(
                "/redirect",
                get(|| async { Redirect::temporary("/secret") }),
            )
            .route("/secret", get(|| async { "must not follow redirects" }))
            .route("/large", get(|| async { vec![b'x'; MAX_BODY + 1] }))
            .route("/ok", get(|| async { "evidence" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = client().unwrap().build().unwrap();
        let token = CancellationToken::new();
        let response = client.get(format!("{base}/redirect")).send().await.unwrap();
        assert!(response.status().is_redirection());
        assert!(body(response, &token).await.is_err());
        assert!(
            body(
                client.get(format!("{base}/large")).send().await.unwrap(),
                &token
            )
            .await
            .is_err()
        );
        assert_eq!(
            body(
                client.get(format!("{base}/ok")).send().await.unwrap(),
                &token
            )
            .await
            .unwrap(),
            b"evidence"
        );
        token.cancel();
        assert!(
            public_client(&Url::parse("http://localhost").unwrap(), &token)
                .await
                .is_err()
        );
        task.abort();
    }
}

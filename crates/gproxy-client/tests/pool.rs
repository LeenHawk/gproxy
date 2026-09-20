#![cfg(all(
    not(target_arch = "wasm32"),
    any(feature = "reqwest", feature = "wreq")
))]

use gproxy_client::{Backend, Client, ClientPool, ConnectionConfig, ProxyConfig, RetryPolicy};

#[path = "support/encoded.rs"]
mod encoded;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Notify, mpsc},
    task::JoinHandle,
};

fn backends() -> Vec<Backend> {
    vec![
        #[cfg(feature = "reqwest")]
        Backend::Reqwest,
        #[cfg(feature = "wreq")]
        Backend::Wreq,
    ]
}

enum Response {
    #[cfg(feature = "reqwest")]
    Reqwest(gproxy_client::reqwest::Response),
    #[cfg(feature = "wreq")]
    Wreq(gproxy_client::wreq::Response),
}

async fn get(client: &Client, url: &str) -> Response {
    try_get(client, url).await.unwrap()
}

async fn try_get(
    client: &Client,
    url: &str,
) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
    Ok(match client {
        #[cfg(feature = "reqwest")]
        Client::Reqwest(client) => Response::Reqwest(client.get(url).send().await?),
        #[cfg(feature = "wreq")]
        Client::Wreq(client) => Response::Wreq(client.get(url).send().await?),
        #[cfg(feature = "reqwest-native")]
        Client::ReqwestNative(_) => unreachable!("not exercised here"),
        Client::Host(_) => unreachable!("this pool builds its own clients"),
    })
}

impl Response {
    fn content_encoding(&self) -> Option<&str> {
        let headers = match self {
            #[cfg(feature = "reqwest")]
            Self::Reqwest(response) => response.headers(),
            #[cfg(feature = "wreq")]
            Self::Wreq(response) => response.headers(),
        };
        headers
            .get("content-encoding")
            .map(|value| value.to_str().unwrap())
    }
    fn status(&self) -> u16 {
        match self {
            #[cfg(feature = "reqwest")]
            Self::Reqwest(response) => response.status().as_u16(),
            #[cfg(feature = "wreq")]
            Self::Wreq(response) => response.status().as_u16(),
        }
    }
    async fn body(self) -> Vec<u8> {
        match self {
            #[cfg(feature = "reqwest")]
            Self::Reqwest(response) => response.bytes().await.unwrap().to_vec(),
            #[cfg(feature = "wreq")]
            Self::Wreq(response) => response.bytes().await.unwrap().to_vec(),
        }
    }
}

struct Server {
    url: String,
    connections: Arc<AtomicUsize>,
    requests: mpsc::UnboundedReceiver<String>,
    release_body: Arc<Notify>,
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let connections = Arc::new(AtomicUsize::new(0));
        let accepted = connections.clone();
        let (tx, requests) = mpsc::unbounded_channel();
        let release_body = Arc::new(Notify::new());
        let release = release_body.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                accepted.fetch_add(1, Ordering::SeqCst);
                let tx = tx.clone();
                let release = release.clone();
                tokio::spawn(async move {
                    loop {
                        let mut request = Vec::new();
                        while !request.ends_with(b"\r\n\r\n") {
                            match stream.read_u8().await {
                                Ok(byte) => request.push(byte),
                                Err(_) => return,
                            }
                        }
                        let request = String::from_utf8(request).unwrap();
                        let slow = request.starts_with("GET /slow ");
                        let redirect = request.starts_with("GET /redirect ");
                        let redirect_loop = request.starts_with("GET /loop ");
                        let unavailable = request.starts_with("GET /unavailable ");
                        let (encoding, body) = encoded::ENCODED
                            .iter()
                            .copied()
                            .find(|(encoding, _)| {
                                request.starts_with(&format!("GET /encoding/{encoding} "))
                            })
                            .unwrap_or(("gzip", b"body"));
                        tx.send(request).unwrap();
                        let status = if redirect {
                            "302 Found\r\nLocation: /followed"
                        } else if redirect_loop {
                            "302 Found\r\nLocation: /loop"
                        } else if unavailable {
                            "503 Service Unavailable"
                        } else {
                            "200 OK"
                        };
                        // Most paths deliberately use non-gzip bytes to check default passthrough.
                        let head = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Encoding: {encoding}\r\n\r\n",
                            body.len()
                        );
                        if stream.write_all(head.as_bytes()).await.is_err() {
                            return;
                        }
                        if slow {
                            release.notified().await;
                        }
                        if stream.write_all(body).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });
        Self {
            url,
            connections,
            requests,
            release_body,
            task,
        }
    }
}

#[tokio::test]
async fn clients_reuse_sockets_and_preserve_transport_responses() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for backend in backends() {
            let mut server = Server::start().await;
            let pool = ClientPool::default();
            let config = ConnectionConfig {
                backend,
                ..Default::default()
            };
            let first = pool.get(&config).await.unwrap();
            for _ in 0..2 {
                let next = pool.get(&config).await.unwrap();
                assert!(Arc::ptr_eq(&first, &next));
                assert_eq!(get(&next, &server.url).await.body().await, b"body");
                server.requests.recv().await.unwrap();
            }
            assert_eq!(server.connections.load(Ordering::SeqCst), 1);
            let response = get(&first, &format!("{}/redirect", server.url)).await;
            assert_eq!(response.status(), 302);
            response.body().await;
            assert!(
                server
                    .requests
                    .recv()
                    .await
                    .unwrap()
                    .starts_with("GET /redirect ")
            );
            assert!(server.requests.try_recv().is_err());

            // The response must arrive before the body; clearing the cache and
            // dropping the client must not buffer, truncate or cancel the stream.
            let response = get(&first, &format!("{}/slow", server.url)).await;
            pool.clear();
            let replacement = pool.get(&config).await.unwrap();
            assert!(!Arc::ptr_eq(&first, &replacement));
            drop(first);
            server.release_body.notify_one();
            assert_eq!(response.body().await, b"body");
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn explicit_proxy_routes_and_authenticates_without_resolving_origin() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for backend in backends() {
            let mut proxy = Server::start().await;
            let pool = ClientPool::default();
            let proxy_url = proxy.url.replacen("http://", "http://alice:secret@", 1);
            let config = ConnectionConfig {
                backend,
                proxy: ProxyConfig::Explicit {
                    url: proxy_url.clone(),
                },
                ..Default::default()
            };
            let client = pool.get(&config).await.unwrap();
            assert_eq!(
                get(&client, "http://origin.invalid/upstream")
                    .await
                    .body()
                    .await,
                b"body"
            );
            let request = proxy.requests.recv().await.unwrap();
            assert!(request.starts_with("GET http://origin.invalid/upstream HTTP/1.1\r\n"));
            assert!(
                request
                    .to_lowercase()
                    .contains("proxy-authorization: basic ywxpy2u6c2vjcmv0\r\n")
            );
            let trailing_slash = ConnectionConfig {
                proxy: ProxyConfig::Explicit {
                    url: format!("{proxy_url}/"),
                },
                ..config.clone()
            };
            assert!(Arc::ptr_eq(
                &client,
                &pool.get(&trailing_slash).await.unwrap()
            ));
            let different_auth = ConnectionConfig {
                proxy: ProxyConfig::Explicit {
                    url: proxy_url.replace("secret", "other"),
                },
                ..config.clone()
            };
            assert!(!Arc::ptr_eq(
                &client,
                &pool.get(&different_auth).await.unwrap()
            ));
            let direct = ConnectionConfig {
                proxy: ProxyConfig::Direct,
                ..config
            };
            assert!(!Arc::ptr_eq(&client, &pool.get(&direct).await.unwrap()));
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_misses_share_a_client_and_parameter_changes_replace_it() {
    for backend in backends() {
        let pool = ClientPool::default();
        let config = ConnectionConfig {
            backend,
            ..Default::default()
        };
        let mut tasks = Vec::new();
        let barrier = Arc::new(tokio::sync::Barrier::new(16));
        for _ in 0..16 {
            let (pool, config, barrier) = (pool.clone(), config.clone(), barrier.clone());
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                pool.get(&config).await.unwrap()
            }));
        }
        let client = tasks.pop().unwrap().await.unwrap();
        for task in tasks {
            assert!(Arc::ptr_eq(&client, &task.await.unwrap()));
        }
        let changed = ConnectionConfig {
            connect_timeout_ms: 5000,
            ..config
        };
        assert!(!Arc::ptr_eq(&client, &pool.get(&changed).await.unwrap()));
    }
}

#[tokio::test]
async fn equivalent_proxy_spelling_and_disabled_idle_pools_share_clients() {
    for backend in backends() {
        let pool = ClientPool::default();
        let config = ConnectionConfig {
            backend,
            proxy: ProxyConfig::Explicit {
                url: "http://EXAMPLE.com:80/".into(),
            },
            pool_idle_timeout_ms: 0,
            ..Default::default()
        };
        let client = pool.get(&config).await.unwrap();
        let mut equivalent = ConnectionConfig {
            backend,
            proxy: ProxyConfig::Explicit {
                url: "http://example.com".into(),
            },
            pool_max_idle_per_host: 0,
            ..Default::default()
        };
        assert!(Arc::ptr_eq(&client, &pool.get(&equivalent).await.unwrap()));
        equivalent.pool_idle_timeout_ms = 0;
        assert!(Arc::ptr_eq(&client, &pool.get(&equivalent).await.unwrap()));
        equivalent.proxy = ProxyConfig::Explicit {
            url: "http://EXAMPLE.com:80/path?query=1#fragment".into(),
        };
        assert!(Arc::ptr_eq(&client, &pool.get(&equivalent).await.unwrap()));
        equivalent.proxy = ProxyConfig::Explicit {
            url: "http://[invalid".into(),
        };
        assert!(matches!(
            pool.get(&equivalent).await.unwrap_err().as_ref(),
            gproxy_client::Error::InvalidProxy(_)
        ));
    }
}

#[tokio::test]
async fn each_decompression_option_controls_bytes_headers_and_client_identity() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for backend in backends() {
            let mut server = Server::start().await;
            let pool = ClientPool::default();
            let base = ConnectionConfig {
                backend,
                // Exercise decompression together with wreq's emulation path.
                emulation: (backend == Backend::Wreq).then(|| {
                    gproxy_client::EmulationConfig::Preset {
                        profile: "chrome_133".into(),
                        platform: "linux".into(),
                        http2: true,
                        headers: false,
                    }
                }),
                ..Default::default()
            };
            let raw = pool.get(&base).await.unwrap();
            for &(encoding, encoded) in encoded::ENCODED {
                let url = format!("{}/encoding/{encoding}", server.url);
                let response = get(&raw, &url).await;
                assert_eq!(response.content_encoding(), Some(encoding));
                assert_eq!(response.body().await, encoded);
                let request = server.requests.recv().await.unwrap().to_lowercase();
                assert!(!request.contains("accept-encoding:"));

                let mut config = base.clone();
                match encoding {
                    "gzip" => config.gzip = true,
                    "br" => config.brotli = true,
                    "deflate" => config.deflate = true,
                    "zstd" => config.zstd = true,
                    _ => unreachable!(),
                }
                let decoded = pool.get(&config).await.unwrap();
                assert!(!Arc::ptr_eq(&raw, &decoded));
                let response = get(&decoded, &url).await;
                assert_eq!(response.content_encoding(), None);
                assert_eq!(response.body().await, encoded::DECODED);
                let request = server.requests.recv().await.unwrap().to_lowercase();
                assert!(
                    request.contains(&format!("accept-encoding: {encoding}\r\n")),
                    "{request}"
                );
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn redirects_follow_the_configured_limit_and_retries_do_not_replay_503() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for backend in backends() {
            let mut server = Server::start().await;
            let pool = ClientPool::default();
            let base = ConnectionConfig {
                backend,
                ..Default::default()
            };
            let original = pool.get(&base).await.unwrap();
            let config = ConnectionConfig {
                redirect_max_hops: 1,
                ..base.clone()
            };
            let follows = pool.get(&config).await.unwrap();
            assert!(!Arc::ptr_eq(&original, &follows));
            let response = get(&follows, &format!("{}/redirect", server.url)).await;
            assert_eq!(response.status(), 200);
            response.body().await;
            assert!(
                server
                    .requests
                    .recv()
                    .await
                    .unwrap()
                    .starts_with("GET /redirect ")
            );
            assert!(
                server
                    .requests
                    .recv()
                    .await
                    .unwrap()
                    .starts_with("GET /followed ")
            );
            assert!(
                try_get(&follows, &format!("{}/loop", server.url))
                    .await
                    .is_err()
            );
            for _ in 0..2 {
                assert!(
                    server
                        .requests
                        .recv()
                        .await
                        .unwrap()
                        .starts_with("GET /loop ")
                );
            }
            assert!(server.requests.try_recv().is_err());

            let retries = pool
                .get(&ConnectionConfig {
                    retry: RetryPolicy::Default,
                    ..base
                })
                .await
                .unwrap();
            assert!(!Arc::ptr_eq(&original, &retries));
            // Backend-default retry only handles safe protocol failures, not 5xx.
            for client in [original, retries] {
                let response = get(&client, &format!("{}/unavailable", server.url)).await;
                assert_eq!(response.status(), 503);
                response.body().await;
                assert!(
                    server
                        .requests
                        .recv()
                        .await
                        .unwrap()
                        .starts_with("GET /unavailable ")
                );
                assert!(server.requests.try_recv().is_err());
            }
        }
    })
    .await
    .unwrap();
}

#[cfg(feature = "wreq")]
#[tokio::test]
async fn fingerprint_is_part_of_client_identity() {
    use gproxy_client::EmulationConfig;
    fn preset(profile: &str, platform: &str, http2: bool, headers: bool) -> EmulationConfig {
        EmulationConfig::Preset {
            profile: profile.into(),
            platform: platform.into(),
            http2,
            headers,
        }
    }
    let pool = ClientPool::default();
    let mut config = ConnectionConfig {
        backend: Backend::Wreq,
        emulation: Some(EmulationConfig::Preset {
            profile: "chrome_133".into(),
            platform: "linux".into(),
            http2: true,
            headers: false,
        }),
        ..Default::default()
    };
    let first = pool.get(&config).await.unwrap();
    config.emulation = Some(preset("firefox_135", "linux", true, false));
    assert!(!Arc::ptr_eq(&first, &pool.get(&config).await.unwrap()));
    config.emulation = Some(preset("chrome_133", "linux", true, true));
    assert!(!Arc::ptr_eq(&first, &pool.get(&config).await.unwrap()));
    config.emulation = Some(EmulationConfig::Custom(gproxy_client::Fingerprint {
        alpn: vec![gproxy_client::Alpn::Http1],
        ..Default::default()
    }));
    assert!(!Arc::ptr_eq(&first, &pool.get(&config).await.unwrap()));
}

// Capture a real ClientHello after an HTTP CONNECT tunnel without depending on
// public providers, test certificates, or a complete TLS server implementation.
async fn tunneled_client_hello(mut config: ConnectionConfig, websocket: bool) -> Vec<u8> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    config.proxy = ProxyConfig::Explicit {
        url: format!("http://alice:secret@{}", listener.local_addr().unwrap()),
    };
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(stream.read_u8().await.unwrap());
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("CONNECT origin.invalid:443 HTTP/1.1\r\n"));
        assert!(
            request
                .to_lowercase()
                .contains("proxy-authorization: basic ywxpy2u6c2vjcmv0\r\n")
        );
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .unwrap();
        let mut header = [0; 5];
        stream.read_exact(&mut header).await.unwrap();
        assert_eq!(header[0], 22, "expected a TLS handshake record");
        let mut hello = vec![0; u16::from_be_bytes([header[3], header[4]]) as usize];
        stream.read_exact(&mut hello).await.unwrap();
        assert_eq!(hello[0], 1, "expected ClientHello");
        hello
    });
    let pool = ClientPool::default();
    let client = if websocket {
        pool.get_websocket(&config).await.unwrap()
    } else {
        pool.get(&config).await.unwrap()
    };
    let failed = match client.as_ref() {
        #[cfg(feature = "reqwest")]
        Client::Reqwest(client) => client
            .get("https://origin.invalid/test")
            .send()
            .await
            .is_err(),
        #[cfg(feature = "reqwest-native")]
        Client::ReqwestNative(client) => client
            .get("https://origin.invalid/test")
            .send()
            .await
            .is_err(),
        #[cfg(feature = "wreq")]
        Client::Wreq(client) => client
            .get("https://origin.invalid/test")
            .send()
            .await
            .is_err(),
        Client::Host(_) => unreachable!("this pool builds its own clients"),
    };
    assert!(failed, "the capture server closes before completing TLS");
    server.await.unwrap()
}

#[tokio::test]
async fn https_uses_authenticated_connect_and_wreq_changes_the_tls_handshake() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for backend in backends() {
            tunneled_client_hello(
                ConnectionConfig {
                    backend,
                    ..Default::default()
                },
                false,
            )
            .await;
        }
        #[cfg(feature = "wreq")]
        {
            // Compare stable cipher suites, excluding GREASE and random/session
            // bytes. This proves the selected profiles reach TLS construction;
            // it is not a browser-fingerprint equivalence claim.
            fn cipher_suites(hello: &[u8]) -> Vec<u16> {
                let offset = 39 + hello[38] as usize;
                let size = u16::from_be_bytes([hello[offset], hello[offset + 1]]) as usize;
                hello[offset + 2..offset + 2 + size]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .copied()
                    .map(u16::from_be_bytes)
                    .filter(|suite| suite & 0x0f0f != 0x0a0a)
                    .collect()
            }
            let mut suites = Vec::new();
            for profile in ["chrome_133", "firefox_135"] {
                let hello = tunneled_client_hello(
                    ConnectionConfig {
                        backend: Backend::Wreq,
                        emulation: Some(gproxy_client::EmulationConfig::Preset {
                            profile: profile.into(),
                            platform: "linux".into(),
                            http2: true,
                            headers: false,
                        }),
                        ..Default::default()
                    },
                    false,
                )
                .await;
                suites.push(cipher_suites(&hello));
            }
            assert_ne!(suites[0], suites[1]);
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn websocket_tls_alpn_is_http1_even_with_emulation() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for backend in backends() {
            let hello = tunneled_client_hello(
                ConnectionConfig {
                    backend,
                    emulation: (backend == Backend::Wreq).then(|| {
                        gproxy_client::EmulationConfig::Preset {
                            profile: "chrome_133".into(),
                            platform: "linux".into(),
                            http2: true,
                            headers: false,
                        }
                    }),
                    ..Default::default()
                },
                true,
            )
            .await;
            let mut offset = 39 + hello[38] as usize;
            offset += 2 + u16::from_be_bytes([hello[offset], hello[offset + 1]]) as usize;
            offset += 1 + hello[offset] as usize; // compression methods
            let end = offset + 2 + u16::from_be_bytes([hello[offset], hello[offset + 1]]) as usize;
            offset += 2;
            let mut alpn = None;
            while offset < end {
                let kind = u16::from_be_bytes([hello[offset], hello[offset + 1]]);
                let length = u16::from_be_bytes([hello[offset + 2], hello[offset + 3]]) as usize;
                offset += 4;
                if kind == 16 {
                    alpn = Some(&hello[offset..offset + length]);
                }
                offset += length;
            }
            assert_eq!(alpn, Some(&b"\x00\x09\x08http/1.1"[..]));
        }
    })
    .await
    .unwrap();
}

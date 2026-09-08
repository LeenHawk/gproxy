use std::sync::Mutex;

use gproxy_admin::dto::RuntimeSettingsDto;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ProxySettings {
    proxy: Option<String>,
    inherit_system_proxy: bool,
}

impl From<&RuntimeSettingsDto> for ProxySettings {
    fn from(settings: &RuntimeSettingsDto) -> Self {
        Self {
            proxy: settings.proxy.clone(),
            inherit_system_proxy: settings.inherit_system_proxy,
        }
    }
}

pub(crate) struct OutboundClient {
    user_agent: &'static str,
    cached: Mutex<Option<(ProxySettings, wreq::Client)>>,
}

impl OutboundClient {
    pub(crate) fn new(user_agent: &'static str) -> Self {
        Self {
            user_agent,
            cached: Mutex::new(None),
        }
    }

    pub(crate) fn get(&self, settings: &RuntimeSettingsDto) -> Result<wreq::Client, wreq::Error> {
        let settings = ProxySettings::from(settings);
        let mut cached = self
            .cached
            .lock()
            .expect("outbound client configuration poisoned");
        if let Some((current, client)) = &*cached
            && *current == settings
        {
            return Ok(client.clone());
        }
        let mut builder = wreq::Client::builder()
            .redirect(wreq::redirect::Policy::limited(10))
            .user_agent(self.user_agent);
        if let Some(proxy) = &settings.proxy {
            builder = builder.proxy(wreq::Proxy::all(proxy)?);
        } else if !settings.inherit_system_proxy {
            builder = builder.no_proxy();
        }
        let client = builder.build()?;
        *cached = Some((settings, client.clone()));
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn auxiliary_clients_follow_proxy_changes_and_can_return_to_direct() {
        let (first, first_task) = proxy("first").await;
        let (second, second_task) = proxy("second").await;
        let client = super::OutboundClient::new("gproxy-runtime-test");
        let mut settings = gproxy_admin::dto::RuntimeSettingsDto::default();
        for (address, expected) in [(first, "first"), (second, "second")] {
            settings.proxy = Some(format!("http://{address}"));
            let body = client
                .get(&settings)
                .unwrap()
                .get("http://upstream.invalid/runtime-probe")
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            assert_eq!(body, expected);
        }
        settings.proxy = None;
        let body = client
            .get(&settings)
            .unwrap()
            .get(format!("http://{first}/runtime-probe"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "first");
        first_task.abort();
        second_task.abort();
    }

    async fn proxy(body: &'static str) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = axum::Router::new().fallback(move || async move { body });
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (address, task)
    }
}

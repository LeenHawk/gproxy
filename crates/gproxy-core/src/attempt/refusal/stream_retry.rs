use serde_json::{Value, json};

use crate::boundary::ByteStream;
use crate::error::CoreError;
use crate::host::Host;

use super::credit;
use super::retry::{Runner, as_stream};

impl<H: Host> Runner<H> {
    pub(super) async fn next_streaming(
        &mut self,
        refused: &Value,
        emitted_output: bool,
    ) -> Result<Option<http::Response<ByteStream>>, CoreError> {
        let Some(mut plan) = self.plan(refused)? else {
            return Ok(None);
        };
        if emitted_output && !plan.continuing {
            return Ok(None);
        }
        loop {
            if plan.carrying_credit && plan.created.elapsed() >= std::time::Duration::from_secs(300)
            {
                if emitted_output || plan.tools {
                    return Ok(None);
                }
                plan.exact
                    .as_object_mut()
                    .expect("request")
                    .remove("fallback_credit_token");
                plan.body = plan.exact.clone();
                plan.carrying_credit = false;
                plan.continuing = false;
            }
            let response = match self.send(&plan.model, &plan.body).await {
                Ok(response) => response,
                Err(CoreError::QuotaExceeded | CoreError::RateLimited { .. }) => return Ok(None),
                Err(error) => return Err(error),
            };
            if response.status() != http::StatusCode::BAD_REQUEST {
                if response.status().is_success() {
                    self.replay.policy.tried.insert(plan.model.clone());
                    self.boundaries.push(json!({"type":"fallback","from":{"model":plan.from},"to":{"model":plan.model}}));
                }
                return Ok(Some(response));
            }
            let response = crate::attempt::body::collect(response)
                .await
                .map_err(|error| CoreError::Transport(error.error))?;
            self.capture(
                response.status(),
                response.headers(),
                response.body().clone(),
            )
            .await;
            let message = credit::message(response.body());
            if plan.carrying_credit
                && message.contains("redemption temporarily unavailable")
                && self.sent < self.replay.budget
            {
                self.core.host.wait(std::time::Duration::from_secs(1)).await;
                continue;
            }
            // A restart would duplicate text already delivered to the client.
            if emitted_output {
                return Ok(None);
            }
            if self.sent >= self.replay.budget {
                return Ok(Some(as_stream(response)));
            }
            if plan.continuing {
                plan.body = plan.exact.clone();
                plan.continuing = false;
            } else if plan.carrying_credit
                && message.contains("fallback_credit_token")
                && !plan.tools
            {
                plan.exact
                    .as_object_mut()
                    .expect("request")
                    .remove("fallback_credit_token");
                plan.body = plan.exact.clone();
                plan.carrying_credit = false;
            } else {
                return Ok(Some(as_stream(response)));
            }
        }
    }
}

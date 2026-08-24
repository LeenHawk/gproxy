mod cycle;
mod quota;

use crate::query::runtime;
use crate::records::{CaptureInput, RequestLogInput};
use crate::{Store, StoreError};

impl Store {
    pub async fn begin_request_log(&self, input: &RequestLogInput) -> Result<(), StoreError> {
        self.backend()
            .execute(runtime::begin_request_log(input)?)
            .await?;
        Ok(())
    }

    pub async fn record_capture(&self, input: &CaptureInput) -> Result<(), StoreError> {
        self.backend()
            .batch(vec![
                runtime::finish_request_log(&input.request_id, input.response_status, None)?,
                runtime::insert_capture(input)?,
            ])
            .await?;
        Ok(())
    }
}

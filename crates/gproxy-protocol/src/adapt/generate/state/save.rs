use super::*;

impl<S: StateStore> GenerationStateAccess<'_, S> {
    pub(in crate::adapt::generate) async fn save_records<N>(
        &self,
        records: Vec<(IdentityStateRecord, Option<super::super::ToolCallKind>)>,
        native_payloads: Vec<(String, Vec<u8>)>,
        progress: &mut GenerationProgress<N>,
    ) -> Result<(), TransformError> {
        if records.len() > self.max_records {
            return Err(limit());
        }
        let mut prepared = Vec::new();
        let mut seen = BTreeSet::new();
        let mut total = 0u64;
        for (record, tool_kind) in records {
            let id = if record.role == IdentityRole::ToolCall {
                record.client_call_id.as_deref()
            } else {
                record.client_item_id.as_deref()
            }
            .ok_or_else(|| {
                TransformError::invalid_result("generation.state", "missing client ID")
            })?;
            let key = self.key(record.role, id)?;
            if !seen.insert(key.clone()) {
                return Err(TransformError::invalid_result(
                    "generation.state",
                    "duplicate client identity",
                ));
            }
            let stored = StoredIdentity {
                schema: 1,
                identity: record,
                tool_kind,
            };

            let bytes = serde_json::to_vec(&stored)?;
            total = total.checked_add(bytes.len() as u64).ok_or_else(limit)?;
            if total > self.store.limits().write_bytes {
                return Err(limit());
            }
            prepared.push((key, bytes));
        }
        for (key, bytes) in native_payloads {
            if !seen.insert(key.clone()) {
                return Err(TransformError::invalid_result(
                    "generation.state",
                    "duplicate native signed record",
                ));
            }
            total = total.checked_add(bytes.len() as u64).ok_or_else(limit)?;
            if total > self.store.limits().write_bytes {
                return Err(limit());
            }
            prepared.push((key, bytes));
        }
        for (key, bytes) in prepared {
            if progress.saved_identities.get(&key) == Some(&bytes) {
                continue;
            }
            self.put(&key, &bytes).await?;
            progress.saved_identities.insert(key, bytes);
        }
        Ok(())
    }
}

impl<S: StateStore> GenerationStateAccess<'_, S> {
    /// Writes one record whatever was there before. A key is the client ID,
    /// and an upstream may repeat its IDs across responses, so an existing
    /// record is the same call seen again or a stale one: never a reason to
    /// fail the response. An identical payload is left alone, and a writer
    /// that lands in between wins, which is as good as this write.
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), TransformError> {
        let write = || {
            Some(StateWrite {
                payload: bytes.to_vec().into(),
                expires_at: Some(self.expires_at),
            })
        };
        if let CasResult::Applied(_) = self
            .store
            .compare_exchange(self.scope, key, None, write())
            .await?
        {
            return Ok(());
        }
        let Some(current) = self.store.get(self.scope, key).await? else {
            // Expired or removed in between; the absent CAS can apply now.
            self.store
                .compare_exchange(self.scope, key, None, write())
                .await?;
            return Ok(());
        };
        if current.payload.as_ref() == bytes && current.expires_at >= Some(self.expires_at) {
            return Ok(());
        }
        self.store
            .compare_exchange(self.scope, key, Some(current.version), write())
            .await?;
        Ok(())
    }
}

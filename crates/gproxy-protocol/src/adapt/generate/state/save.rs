use super::*;
impl<S: StateStore> GenerationStateAccess<'_, S> {
    pub(in crate::adapt::generate) async fn save_records<N>(
        &self,
        records: Vec<(IdentityStateRecord, Option<super::super::ToolCallKind>)>,
        native_payloads: Vec<(String, Vec<u8>)>,
        progress: &mut GenerationProgress<N>,
    ) -> Result<(), TransformError> {
        self.save_records_with_chat_forms(records, native_payloads, &BTreeMap::new(), progress)
            .await
    }
    pub(super) async fn save_records_with_chat_forms<N>(
        &self,
        records: Vec<(IdentityStateRecord, Option<super::super::ToolCallKind>)>,
        native_payloads: Vec<(String, Vec<u8>)>,
        chat_forms: &BTreeMap<String, super::ChatCallForm>,
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
            let chat_form = if record.role == IdentityRole::ToolCall {
                chat_forms.get(id).copied()
            } else {
                None
            };
            let stored = StoredIdentity {
                schema: 1,
                identity: record,
                tool_kind,
                chat_form,
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
        if let Some((key, bytes)) = &progress.pending_identity_write
            && !prepared.iter().any(|(k, v)| k == key && v == bytes)
        {
            return Err(TransformError::new(
                TransformErrorKind::Conflict,
                "generation.state",
                "unacknowledged identity write differs from recovery facts",
            ));
        }
        for (key, bytes) in prepared {
            if let Some((version, existing)) = progress.saved_identities.get(&key) {
                if existing != &bytes {
                    return Err(TransformError::new(
                        TransformErrorKind::Conflict,
                        "generation.state",
                        "identity changed during response recovery",
                    ));
                }
                let current = self.store.get(self.scope, &key).await?.ok_or_else(|| {
                    TransformError::new(
                        TransformErrorKind::MissingState,
                        "generation.state",
                        "previously applied record expired or disappeared",
                    )
                })?;
                if current.payload.len() as u64 > self.store.limits().read_bytes {
                    return Err(limit());
                }
                if &current.version != version
                    || current.payload.as_ref() != existing.as_slice()
                    || current.expires_at != Some(self.expires_at)
                {
                    return Err(TransformError::new(
                        TransformErrorKind::Conflict,
                        "generation.state",
                        "previously applied record changed",
                    ));
                }
                continue;
            }
            if let Some((pending_key, pending_bytes)) = &progress.pending_identity_write {
                if pending_key != &key || pending_bytes != &bytes {
                    return Err(TransformError::new(
                        TransformErrorKind::Conflict,
                        "generation.state",
                        "pending identity order changed",
                    ));
                }
                let current = self.store.get(self.scope, &key).await?.ok_or_else(|| {
                    TransformError::new(
                        TransformErrorKind::MissingState,
                        "generation.state",
                        "unacknowledged identity CAS has no durable result; cannot retry",
                    )
                })?;
                if current.payload.len() as u64 > self.store.limits().read_bytes {
                    return Err(limit());
                }
                if current.payload.as_ref() != bytes.as_slice()
                    || current.expires_at != Some(self.expires_at)
                {
                    return Err(TransformError::new(
                        TransformErrorKind::Conflict,
                        "generation.state",
                        "unacknowledged identity CAS differs from durable state",
                    ));
                }
                progress
                    .saved_identities
                    .insert(key, (current.version, bytes));
                progress.pending_identity_write = None;
                continue;
            }
            progress.pending_identity_write = Some((key.clone(), bytes.clone()));
            let result = self
                .store
                .compare_exchange(
                    self.scope,
                    &key,
                    None,
                    Some(StateWrite {
                        payload: bytes.clone().into(),
                        expires_at: Some(self.expires_at),
                    }),
                )
                .await?;
            progress.pending_identity_write = None;
            match result {
                CasResult::Applied(Some(version)) => {
                    progress.saved_identities.insert(key, (version, bytes));
                }
                _ => {
                    return Err(TransformError::new(
                        TransformErrorKind::Conflict,
                        "generation.state",
                        "client identity already exists or CAS was not applied",
                    ));
                }
            }
        }
        Ok(())
    }
}

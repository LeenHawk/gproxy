use super::{
    items::{Item, item_id},
    *,
};

impl ResponsesStreamCollector {
    pub(super) fn seed_output(
        &mut self,
        output: &[r::ResponseOutputItem],
    ) -> Result<(), TransformError> {
        for value in output {
            if self.items.len() >= self.limits.max_items {
                return Err(limit());
            }
            if let Some(id) = item_id(value)
                && (id.is_empty() || !self.ids.insert(id.to_owned()))
            {
                return Err(invalid("snapshot contains empty or duplicate item ID"));
            }
            let (text, json) = super::lifecycle::item_sizes(value);
            self.charge(text, false)?;
            self.charge(json, true)?;
            self.items.push(Item::new(value.clone()));
        }
        Ok(())
    }
    pub(super) fn snapshot_output(
        &self,
        output: &[r::ResponseOutputItem],
    ) -> Result<(), TransformError> {
        if output.is_empty() {
            return Ok(());
        }
        if output.len() != self.items.len() {
            return Err(invalid(
                "nonterminal snapshot output count differs from observed item lifecycle",
            ));
        }
        for (observed, snapshot) in self.items.iter().zip(output) {
            if observed.done {
                if &observed.value != snapshot {
                    return Err(invalid("snapshot changed completed item"));
                }
                continue;
            }
            let mut expected = observed.value.clone();
            for ((summary, index), part) in &observed.parts {
                let index =
                    usize::try_from(*index).map_err(|_| invalid("negative snapshot part index"))?;
                match (&mut expected, &part.value, *summary) {
                    (
                        r::ResponseOutputItem::Message(value),
                        s::OutputContentPart::Text(part),
                        false,
                    ) => {
                        if index != value.content.len() {
                            return Err(invalid("snapshot content index mismatch"));
                        }
                        value.content.push(i::OutputContent::Text(part.clone()));
                    }
                    (
                        r::ResponseOutputItem::Message(value),
                        s::OutputContentPart::Refusal(part),
                        false,
                    ) => {
                        if index != value.content.len() {
                            return Err(invalid("snapshot content index mismatch"));
                        }
                        value.content.push(i::OutputContent::Refusal(part.clone()));
                    }
                    (
                        r::ResponseOutputItem::Reasoning(value),
                        s::OutputContentPart::Reasoning(part),
                        true,
                    ) => {
                        if index != value.summary.len() {
                            return Err(invalid("snapshot summary index mismatch"));
                        }
                        value.summary.push(
                            i::SummaryText::builder(
                                i::SummaryTextType::SummaryText,
                                part.text.clone(),
                            )
                            .build(),
                        );
                    }
                    (
                        r::ResponseOutputItem::Reasoning(value),
                        s::OutputContentPart::Reasoning(part),
                        false,
                    ) => {
                        let content = value.content.get_or_insert_with(Vec::new);
                        if index != content.len() {
                            return Err(invalid("snapshot reasoning index mismatch"));
                        }
                        content.push(
                            i::ReasoningContent::builder(
                                i::ReasoningTextType::ReasoningText,
                                part.text.clone(),
                            )
                            .build(),
                        );
                    }
                    _ => return Err(invalid("snapshot part type mismatch")),
                }
            }
            if &expected != snapshot {
                return Err(invalid(
                    "nonterminal snapshot contradicts accumulated native items",
                ));
            }
        }
        Ok(())
    }
}

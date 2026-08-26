use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Dedupe {
    responses: BTreeSet<String>,
    transcriptions: BTreeSet<(String, Option<u32>)>,
}

impl Dedupe {
    pub(super) fn response_seen(&self, id: &str) -> bool {
        self.responses.contains(id)
    }

    pub(super) fn record_response(&mut self, id: &str) {
        self.responses.insert(id.into());
    }

    pub(super) fn transcription_seen(&self, id: &(String, Option<u32>)) -> bool {
        self.transcriptions.contains(id)
    }

    pub(super) fn record_transcription(&mut self, id: (String, Option<u32>)) {
        self.transcriptions.insert(id);
    }
}

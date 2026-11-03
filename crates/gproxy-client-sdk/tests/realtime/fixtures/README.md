These saved offline traces use documented OpenAI Realtime GA and Gemini Live
message shapes, with deterministic IDs, usage and tiny synthetic audio payloads.
They are scripted fixtures, not recordings of paid upstream sessions.

Sources: `upstream_docs/openai/docs/Realtime.md` and
`upstream_docs/gemini/docs/Live API reference.md`, checked against protocol DTOs.
Tests splice interruption, tool, failure and extension frames into these traces.

use gproxy_protocol::{claude, gemini, openai, wire};

fn same_claude(_: claude::generate_content::GenerateContentRequestBody) {}
fn same_gemini(_: gemini::GenerateContentRequestBody) {}
fn same_openai(_: openai::responses::GenerateContentRequestBody) {}

#[test]
fn legacy_and_wire_paths_name_the_same_types() {
    let _: fn(wire::claude::generate_content::GenerateContentRequestBody) = same_claude;
    let _: fn(wire::gemini::GenerateContentRequestBody) = same_gemini;
    let _: fn(wire::openai::responses::GenerateContentRequestBody) = same_openai;
}

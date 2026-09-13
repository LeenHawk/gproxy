use gproxy_protocol_macros::WireBuilder;

#[derive(WireBuilder)]
#[non_exhaustive]
struct Request<T>
where
    T: Clone,
{
    model: String,
    input: T,
    limit: Option<u32>,
    r#type: std::option::Option<String>,
    rest: std::collections::BTreeMap<String, String>,
}

#[test]
fn required_arguments_optional_setters_and_extension_fields() {
    let request = Request::builder("model".into(), vec![1, 2])
        .limit(12_u32)
        .r#type("text")
        .rest([("future".into(), "value".into())].into())
        .build();
    assert_eq!(request.model, "model");
    assert_eq!(request.input, [1, 2]);
    assert_eq!(request.limit, Some(12));
    assert_eq!(request.r#type.as_deref(), Some("text"));
    assert_eq!(request.rest["future"], "value");

    let minimal = Request::builder("model".into(), ()).build();
    assert_eq!(minimal.limit, None);
    assert_eq!(minimal.r#type, None);
    assert!(minimal.rest.is_empty());
}

#[derive(WireBuilder)]
struct NullableRequest {
    name: String,
    #[wire(required)]
    parameters: Option<String>,
    description: Option<String>,
}

#[test]
fn required_nullable_argument_is_supplied_even_when_null() {
    let request = NullableRequest::builder("lookup".into(), None)
        .description("description")
        .build();
    assert_eq!(request.name, "lookup");
    assert_eq!(request.parameters, None);
    assert_eq!(request.description.as_deref(), Some("description"));

    let request = NullableRequest::builder("lookup".into(), Some("schema".into())).build();
    assert_eq!(request.parameters.as_deref(), Some("schema"));
    assert_eq!(request.description, None);
}

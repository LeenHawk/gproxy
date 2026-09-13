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

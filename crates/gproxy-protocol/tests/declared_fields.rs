use gproxy_protocol::wire::DeclaredFields;
use gproxy_protocol_macros::DeclaredFields;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

// Deliberately neither Clone, Serialize nor DeclaredFields. An extension bag
// must be discarded, not inspected through a serialization/clone workaround.
#[derive(Default)]
struct OpaqueExtension {
    marker: bool,
}

#[derive(DeclaredFields)]
struct Generic<T> {
    value: T,
    next: Option<Box<Generic<T>>>,
    #[declared(extension)]
    extension: OpaqueExtension,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, DeclaredFields)]
struct Node {
    text: String,
    schema: Value,
    metadata: Map<String, Value>,
    child: Option<Box<Node>>,
    nullable: Option<Option<String>>,
    #[serde(default, flatten)]
    rest: Map<String, Value>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, DeclaredFields)]
enum Message {
    Unit,
    Tuple(Vec<Node>),
    Named {
        body: Box<Node>,
        #[serde(default, flatten)]
        rest: Map<String, Value>,
    },
}

fn node() -> Node {
    Node {
        text: "visible".into(),
        schema: json!({"properties":{"rest":{"type":"string"}},"future_keyword":true}),
        metadata: Map::from_iter([("foreign-looking".into(), json!({"unknown": 4}))]),
        child: None,
        nullable: Some(None),
        rest: Map::from_iter([("foreign-extension".into(), json!(true))]),
    }
}

#[test]
fn recursive_structs_preserve_formal_json_and_presence_without_extensions() {
    let mut input = node();
    input.child = Some(Box::new(node()));
    let before_schema = input.schema.clone();
    let before_metadata = input.metadata.clone();
    let output = input.into_declared();
    assert!(output.rest.is_empty());
    assert!(output.child.as_ref().unwrap().rest.is_empty());
    assert_eq!(output.schema, before_schema);
    assert_eq!(output.metadata, before_metadata);
    assert_eq!(output.nullable, Some(None));
    assert_eq!(output.child.as_ref().unwrap().text, "visible");
    let wire = serde_json::to_value(output).unwrap();
    assert!(wire.get("foreign-extension").is_none());
    assert_eq!(wire["schema"]["future_keyword"], true);
}

#[test]
fn every_enum_shape_rebuilds_the_original_type() {
    assert_eq!(Message::Unit.into_declared(), Message::Unit);
    let Message::Tuple(nodes) = Message::Tuple(vec![node()]).into_declared() else {
        panic!("variant changed")
    };
    assert!(nodes[0].rest.is_empty());
    let Message::Named { body, rest } = (Message::Named {
        body: Box::new(node()),
        rest: Map::from_iter([("opaque".into(), json!(42))]),
    })
    .into_declared() else {
        panic!("variant changed")
    };
    assert!(rest.is_empty());
    assert!(body.rest.is_empty());
}

#[test]
fn generic_recursive_extension_is_never_cloned_or_serialized() {
    let value = Generic {
        value: "outer".to_owned(),
        next: Some(Box::new(Generic {
            value: "inner".to_owned(),
            next: None,
            extension: OpaqueExtension { marker: true },
        })),
        extension: OpaqueExtension { marker: true },
    }
    .into_declared();
    assert_eq!(value.value, "outer");
    assert!(!value.extension.marker);
    let inner = value.next.unwrap();
    assert_eq!(inner.value, "inner");
    assert!(!inner.extension.marker);
}

#[test]
fn alternate_crate_path_unit_and_conditional_fields_are_supported() {
    use gproxy_protocol as protocol;
    #[derive(DeclaredFields)]
    #[declared(crate = "protocol")]
    struct Unit;
    #[derive(DeclaredFields)]
    #[declared(crate = "protocol")]
    struct Conditional {
        text: String,
        #[cfg(any())]
        absent: UndefinedType,
    }
    let _ = Unit.into_declared();
    assert_eq!(
        Conditional {
            text: "kept".into()
        }
        .into_declared()
        .text,
        "kept"
    );
}

#[test]
fn generic_bounds_follow_field_types_and_extension_defaults() {
    use std::marker::PhantomData;
    trait Source {
        type Item;
    }
    struct Opaque;
    impl Source for Opaque {
        type Item = Node;
    }
    #[derive(DeclaredFields)]
    struct Associated<T: Source> {
        value: T::Item,
        marker: PhantomData<T>,
        next: Option<Box<Associated<T>>>,
    }
    #[derive(DeclaredFields)]
    struct Extension<T> {
        #[declared(extension)]
        extension: T,
    }
    let output = Associated::<Opaque> {
        value: node(),
        marker: PhantomData,
        next: None,
    }
    .into_declared();
    assert!(output.value.rest.is_empty());
    let output = Extension {
        extension: OpaqueExtension { marker: true },
    }
    .into_declared();
    assert!(!output.extension.marker);
}

#[test]
fn hash_map_keeps_non_default_non_clone_hasher_state() {
    use std::{
        collections::HashMap,
        hash::{BuildHasher, DefaultHasher},
    };
    struct StatefulHasher(u64);
    impl BuildHasher for StatefulHasher {
        type Hasher = DefaultHasher;
        fn build_hasher(&self) -> Self::Hasher {
            DefaultHasher::new()
        }
    }
    let mut input = HashMap::with_hasher(StatefulHasher(123));
    input.insert("key", node());
    let output = input.into_declared();
    assert_eq!(output.hasher().0, 123);
    assert!(output["key"].rest.is_empty());
    assert_eq!(output["key"].text, "visible");
}

#[test]
fn explicit_bounds_support_mutually_recursive_generics() {
    #[derive(DeclaredFields)]
    #[declared(bound = "T: gproxy_protocol::wire::DeclaredFields")]
    struct First<T> {
        value: T,
        next: Option<Box<Second<T>>>,
    }
    #[derive(DeclaredFields)]
    #[declared(bound = "T: gproxy_protocol::wire::DeclaredFields")]
    struct Second<T> {
        next: Option<Box<First<T>>>,
    }
    let output = First {
        value: node(),
        next: Some(Box::new(Second {
            next: Some(Box::new(First {
                value: node(),
                next: None,
            })),
        })),
    }
    .into_declared();
    assert!(output.value.rest.is_empty());
    assert!(output.next.unwrap().next.unwrap().value.rest.is_empty());
}

#[test]
fn concrete_mutual_recursion_needs_no_extra_bounds() {
    #[derive(DeclaredFields)]
    struct Schema {
        additional: Option<Box<Additional>>,
        #[declared(extension)]
        extension: OpaqueExtension,
    }
    #[derive(DeclaredFields)]
    enum Additional {
        Schema(Schema),
        Boolean(bool),
    }
    let input = Schema {
        additional: Some(Box::new(Additional::Schema(Schema {
            additional: Some(Box::new(Additional::Boolean(true))),
            extension: OpaqueExtension { marker: true },
        }))),
        extension: OpaqueExtension { marker: true },
    };
    let output = input.into_declared();
    assert!(!output.extension.marker);
    let Additional::Schema(inner) = *output.additional.unwrap() else {
        panic!("schema variant")
    };
    assert!(!inner.extension.marker);
    assert!(matches!(
        *inner.additional.unwrap(),
        Additional::Boolean(true)
    ));
}

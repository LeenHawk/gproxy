use gproxy_protocol::codec::{
    CodecErrorKind, CodecLimits, JsonArrayDecoder, JsonArrayEncoder, JsonDecoder, NdjsonDecoder,
    NdjsonEncoder,
};
use serde_json::json;

fn limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 1024,
        max_value_bytes: 512,
        max_body_bytes: 4096,
        max_line_bytes: 512,
        max_part_bytes: 1024,
        max_parts: 8,
    }
}

#[test]
fn json_array_handles_nested_values_strings_and_split_utf8() {
    let mut decoder = JsonArrayDecoder::new(limits());
    let mut values = Vec::new();
    for chunk in [
        b"[{\"text\":\"a,".as_slice(),
        b"b\",\"nested\":[".as_slice(),
        "1,2]}]".as_bytes(),
    ] {
        values.extend(decoder.push(chunk).unwrap());
    }
    decoder.finish().unwrap();
    assert_eq!(values, [json!({"text": "a,b", "nested": [1, 2]})]);

    let mut decoder = JsonArrayDecoder::new(limits());
    let bytes = "[\"€\"]".as_bytes();
    let values = [
        decoder.push(&bytes[..3]).unwrap(),
        decoder.push(&bytes[3..]).unwrap(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    decoder.finish().unwrap();
    assert_eq!(values, [json!("€")]);
}

#[test]
fn json_and_ndjson_reject_truncation_invalid_utf8_and_limits() {
    let mut json_decoder = JsonDecoder::new(limits());
    json_decoder.push(b"{\"x\":").unwrap();
    assert_eq!(
        json_decoder
            .finish::<serde_json::Value>()
            .unwrap_err()
            .kind(),
        CodecErrorKind::UnexpectedEof
    );

    let mut array = JsonArrayDecoder::new(limits());
    array.push(b"[{\"x\":1").unwrap();
    assert_eq!(
        array.finish().unwrap_err().kind(),
        CodecErrorKind::UnexpectedEof
    );
    let mut trailing = JsonArrayDecoder::new(limits());
    assert_eq!(
        trailing.push(b"[1,]").unwrap_err().kind(),
        CodecErrorKind::Invalid
    );
    let mut split_number = JsonArrayDecoder::new(limits());
    assert!(split_number.push(b"[1").unwrap().is_empty());
    assert_eq!(split_number.push(b"2]").unwrap(), [json!(12)]);
    split_number.finish().unwrap();

    let mut ndjson = NdjsonDecoder::new(limits());
    let values = ndjson.push(b"{\"x\":\"a").unwrap();
    assert!(values.is_empty());
    let values = ndjson.push("b\"}\n".as_bytes()).unwrap();
    assert_eq!(values, [json!({"x": "ab"})]);

    let mut invalid = JsonDecoder::new(limits());
    invalid.push(&[0xff]).unwrap();
    assert!(matches!(
        invalid.finish::<serde_json::Value>().unwrap_err().kind(),
        CodecErrorKind::Json | CodecErrorKind::Utf8
    ));

    let mut tiny = JsonDecoder::new(CodecLimits {
        max_buffer_bytes: 2,
        ..limits()
    });
    assert_eq!(tiny.push(b"123").unwrap_err().kind(), CodecErrorKind::Limit);
}

#[test]
fn json_encoders_emit_bounded_array_and_ndjson_frames() {
    let mut array = JsonArrayEncoder::new(limits());
    let mut output = Vec::new();
    output.extend_from_slice(&array.push(&json!({"x": 1})).unwrap());
    output.extend_from_slice(&array.push(&json!("two")).unwrap());
    output.extend_from_slice(&array.finish().unwrap());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output).unwrap(),
        json!([{"x": 1}, "two"])
    );

    let line = NdjsonEncoder::new(limits())
        .encode(&json!({"ok": true}))
        .unwrap();
    assert_eq!(line.last(), Some(&b'\n'));
}

#[test]
fn json_array_is_invariant_at_every_byte_split_including_scalar_boundaries() {
    let wire = "[12,-3.25e+2,true,false,null,\"€\\\"text\",{\"x\":[1,2]},[]]".as_bytes();
    let expected: serde_json::Value = serde_json::from_slice(wire).unwrap();
    for split in 0..=wire.len() {
        let mut d = JsonArrayDecoder::new(limits());
        let mut out = d.push(&wire[..split]).unwrap();
        out.extend(d.push(&wire[split..]).unwrap());
        d.finish().unwrap();
        assert_eq!(out, expected.as_array().unwrap().clone(), "split {split}");
    }
    let mut d = JsonArrayDecoder::new(limits());
    let mut out = vec![];
    for b in wire {
        out.extend(d.push(&[*b]).unwrap());
    }
    d.finish().unwrap();
    assert_eq!(out, expected.as_array().unwrap().clone());
    for invalid in [
        b"[1,]".as_slice(),
        b"[,1]",
        b"[1 2]",
        b"[01]",
        b"[1e]",
        b"[truefalse]",
        b"[\x0b]",
        b"[{}]extra",
    ] {
        let mut d = JsonArrayDecoder::new(limits());
        assert!(
            d.push(invalid).and_then(|_| d.finish()).is_err(),
            "{invalid:?}"
        );
    }
}
#[test]
fn incremental_json_limits_measure_values_and_body_not_transport_chunk_size() {
    let small = CodecLimits {
        max_buffer_bytes: 8,
        max_value_bytes: 8,
        max_line_bytes: 8,
        max_body_bytes: 100,
        ..limits()
    };
    let mut d = JsonArrayDecoder::new(small);
    let out = d.push(b"[1,2,3,4,5,6,7,8,9,10]").unwrap();
    assert_eq!(out.len(), 10);
    d.finish().unwrap();
    let mut d = JsonArrayDecoder::new(CodecLimits {
        max_body_bytes: 5,
        ..small
    });
    assert!(d.push(b"[1,2]").is_ok());
    assert_eq!(d.push(b" ").unwrap_err().kind(), CodecErrorKind::Limit);
    assert!(d.finish().is_err());
    let mut d = JsonDecoder::new(CodecLimits {
        max_body_bytes: 2,
        ..limits()
    });
    assert_eq!(d.push(b"123").unwrap_err().kind(), CodecErrorKind::Limit);
    assert!(d.finish::<serde_json::Value>().is_err());
    let mut d = NdjsonDecoder::new(CodecLimits {
        max_buffer_bytes: 2,
        ..limits()
    });
    assert!(d.push(b"123").is_err());
    let mut d = NdjsonDecoder::new(CodecLimits {
        max_body_bytes: 4,
        ..limits()
    });
    assert_eq!(d.push(b"1\n2\n").unwrap().len(), 2);
    assert!(d.push(b"3").is_err());
    assert!(d.finish().is_err());
    let mut encoder = JsonArrayEncoder::new(CodecLimits {
        max_body_bytes: 3,
        ..limits()
    });
    assert_eq!(encoder.push(&json!(1)).unwrap(), b"[1".as_slice());
    assert!(encoder.push(&json!(2)).is_err());
    assert!(encoder.finish().is_err());
    let mut encoder = NdjsonEncoder::new(CodecLimits {
        max_body_bytes: 2,
        ..limits()
    });
    assert_eq!(encoder.encode(&json!(1)).unwrap(), b"1\n".as_slice());
    assert!(encoder.encode(&json!(2)).is_err());
}
#[test]
fn json_encoder_stops_serializing_at_the_limit_before_unbounded_allocation() {
    struct Infinite;
    impl serde::Serialize for Infinite {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeSeq;
            let mut seq = serializer.serialize_seq(None)?;
            for _ in 0..1000 {
                seq.serialize_element(&"0123456789")?;
            }
            panic!("serializer was not stopped by bounded writer")
        }
    }
    assert_eq!(
        gproxy_protocol::codec::encode_json(
            &Infinite,
            CodecLimits {
                max_value_bytes: 30,
                ..limits()
            }
        )
        .unwrap_err()
        .kind(),
        CodecErrorKind::Limit
    );
}

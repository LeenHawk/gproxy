use bytes::Bytes;
use serde_json::Value;

use crate::ResponseStream;

pub(super) fn bytes_text(frames: &[Bytes]) -> String {
    String::from_utf8_lossy(
        &frames
            .iter()
            .flat_map(|frame| frame.iter().copied())
            .collect::<Vec<_>>(),
    )
    .into_owned()
}

pub(super) fn drive(mut stream: ResponseStream, wire: &str, chunk: usize) -> Vec<u8> {
    let mut output = Vec::new();
    for part in wire.as_bytes().chunks(chunk) {
        for frame in stream.push(Bytes::copy_from_slice(part)).unwrap() {
            output.extend_from_slice(&frame);
        }
    }
    for frame in stream.finish().unwrap() {
        output.extend_from_slice(&frame);
    }
    output
}

pub(super) fn data_frames(wire: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(wire)
        .split("\n\n")
        .filter_map(|frame| {
            let data = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .collect::<Vec<_>>()
                .join("\n");
            serde_json::from_str(&data).ok()
        })
        .collect()
}

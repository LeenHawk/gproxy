//! The protobuf wire format, as much of it as this channel needs.
//!
//! Six writers (varint, length-delimited string, bytes, embedded message,
//! fixed64 double) and a flat field reader. Nothing here knows a schema: the
//! request builder names its own field numbers and the response reader looks
//! them up, which is what reverse-engineered coordinates require — a field the
//! upstream added is an unknown number that parses and is ignored, never a
//! decode failure.
//!
//! Shape follows `samples/windsurfapi/src/proto.js` (a zero-dependency
//! JavaScript writer/parser for the same upstream); the code is this crate's.
//! See the wire-type table in the protobuf encoding reference: 0 varint,
//! 1 fixed64, 2 length-delimited, 5 fixed32.

use crate::channel::ChannelError;

const WIRE_VARINT: u32 = 0;
const WIRE_FIXED64: u32 = 1;
const WIRE_LENGTH: u32 = 2;
const WIRE_FIXED32: u32 = 5;

/// A protobuf message under construction. Fields are appended in call order;
/// the upstream's decoder does not require ascending numbers, but every
/// builder here writes them in order anyway so a capture reads like the
/// reference one.
#[derive(Debug, Default, Clone)]
pub struct Message {
    bytes: Vec<u8>,
}

impl Message {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wire type 0. Enums and `bool` (as 0/1) are varints too.
    pub fn varint(&mut self, field: u32, value: u64) -> &mut Self {
        self.tag(field, WIRE_VARINT);
        self.write_varint(value);
        self
    }

    /// Wire type 2 carrying UTF-8. An empty value still emits the field:
    /// `GetChatMessageRequest.system_prompt` is present-but-empty on the
    /// reference wire when the caller sent no system turn.
    pub fn string(&mut self, field: u32, value: &str) -> &mut Self {
        self.bytes(field, value.as_bytes())
    }

    /// Wire type 2 carrying opaque bytes.
    pub fn bytes(&mut self, field: u32, value: &[u8]) -> &mut Self {
        self.tag(field, WIRE_LENGTH);
        self.write_varint(value.len() as u64);
        self.bytes.extend_from_slice(value);
        self
    }

    /// Wire type 2 carrying another message. Repeated fields are written by
    /// calling this once per element with the same number.
    pub fn message(&mut self, field: u32, value: &Message) -> &mut Self {
        self.bytes(field, value.as_bytes())
    }

    /// Wire type 1 carrying an IEEE-754 double, little-endian.
    pub fn double(&mut self, field: u32, value: f64) -> &mut Self {
        self.tag(field, WIRE_FIXED64);
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn tag(&mut self, field: u32, wire: u32) {
        self.write_varint(u64::from(field) << 3 | u64::from(wire));
    }

    fn write_varint(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(byte);
                return;
            }
            self.bytes.push(byte | 0x80);
        }
    }
}

/// One decoded field. `Bytes` borrows the buffer, so a sub-message is parsed
/// by handing its slice back to [`parse`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    Varint(u64),
    Fixed64([u8; 8]),
    Bytes(&'a [u8]),
    Fixed32([u8; 4]),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Field<'a> {
    pub number: u32,
    pub value: Value<'a>,
}

fn malformed(detail: &str) -> ChannelError {
    ChannelError::InvalidResponse(format!("devin protobuf: {detail}"))
}

fn read_varint(bytes: &[u8], offset: &mut usize) -> Result<u64, ChannelError> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    loop {
        let byte = *bytes
            .get(*offset)
            .ok_or_else(|| malformed("truncated varint"))?;
        *offset += 1;
        value |= u64::from(byte & 0x7f)
            .checked_shl(shift)
            .ok_or_else(|| malformed("varint overflows 64 bits"))?;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
        if shift >= 64 {
            return Err(malformed("varint overflows 64 bits"));
        }
    }
}

fn take<'a>(bytes: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8], ChannelError> {
    let end = offset
        .checked_add(len)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| malformed("field runs past the end of the message"))?;
    let slice = &bytes[*offset..end];
    *offset = end;
    Ok(slice)
}

/// Every field of one message, in wire order and including repeats. Groups
/// (wire types 3 and 4) are rejected: the upstream does not use them and a
/// buffer that claims to is not the message we think it is.
pub fn parse(bytes: &[u8]) -> Result<Vec<Field<'_>>, ChannelError> {
    let mut fields = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let tag = read_varint(bytes, &mut offset)?;
        let number = u32::try_from(tag >> 3).map_err(|_| malformed("field number overflow"))?;
        if number == 0 {
            return Err(malformed("field number 0 is not valid"));
        }
        let value = match (tag & 0x07) as u32 {
            WIRE_VARINT => Value::Varint(read_varint(bytes, &mut offset)?),
            WIRE_FIXED64 => {
                let slice = take(bytes, &mut offset, 8)?;
                let mut out = [0_u8; 8];
                out.copy_from_slice(slice);
                Value::Fixed64(out)
            }
            WIRE_LENGTH => {
                let len = read_varint(bytes, &mut offset)?;
                let len = usize::try_from(len).map_err(|_| malformed("length overflow"))?;
                Value::Bytes(take(bytes, &mut offset, len)?)
            }
            WIRE_FIXED32 => {
                let slice = take(bytes, &mut offset, 4)?;
                let mut out = [0_u8; 4];
                out.copy_from_slice(slice);
                Value::Fixed32(out)
            }
            other => return Err(malformed(&format!("unsupported wire type {other}"))),
        };
        fields.push(Field { number, value });
    }
    Ok(fields)
}

/// The first length-delimited value of this field number.
pub fn bytes_of<'a>(fields: &[Field<'a>], number: u32) -> Option<&'a [u8]> {
    fields.iter().find_map(|field| match field.value {
        Value::Bytes(bytes) if field.number == number => Some(bytes),
        _ => None,
    })
}

/// The first varint value of this field number.
pub fn varint_of(fields: &[Field<'_>], number: u32) -> Option<u64> {
    fields.iter().find_map(|field| match field.value {
        Value::Varint(value) if field.number == number => Some(value),
        _ => None,
    })
}

/// The first length-delimited value of this field number, as UTF-8. A field
/// that is not valid UTF-8 reads as absent rather than failing the frame.
pub fn text_of<'a>(fields: &[Field<'a>], number: u32) -> Option<&'a str> {
    bytes_of(fields, number).and_then(|bytes| std::str::from_utf8(bytes).ok())
}

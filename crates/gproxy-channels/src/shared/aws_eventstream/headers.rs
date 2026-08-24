use gproxy_channel_api::ChannelError;

use super::decode as error;

#[derive(Default)]
pub(super) struct Fields {
    pub(super) message_type: Option<String>,
    pub(super) event_type: Option<String>,
    pub(super) exception_type: Option<String>,
    pub(super) content_type: Option<String>,
}

pub(super) fn decode(mut bytes: &[u8]) -> Result<Fields, ChannelError> {
    let mut fields = Fields::default();
    while !bytes.is_empty() {
        let name_len = take(&mut bytes, 1)?[0] as usize;
        let name = std::str::from_utf8(take(&mut bytes, name_len)?)
            .map_err(|_| error("header name is not UTF-8"))?;
        let kind = take(&mut bytes, 1)?[0];
        let value = decode_value(kind, &mut bytes)?;
        let Some(value) = value else {
            continue;
        };
        match name {
            ":message-type" => fields.message_type = Some(value.to_owned()),
            ":event-type" => fields.event_type = Some(value.to_owned()),
            ":exception-type" => fields.exception_type = Some(value.to_owned()),
            ":content-type" => fields.content_type = Some(value.to_owned()),
            _ => {}
        }
    }
    Ok(fields)
}

fn decode_value<'a>(kind: u8, bytes: &mut &'a [u8]) -> Result<Option<&'a str>, ChannelError> {
    match kind {
        0 | 1 => Ok(None),
        2 => {
            take(bytes, 1)?;
            Ok(None)
        }
        3 => {
            take(bytes, 2)?;
            Ok(None)
        }
        4 => {
            take(bytes, 4)?;
            Ok(None)
        }
        5 | 8 => {
            take(bytes, 8)?;
            Ok(None)
        }
        6 | 7 => {
            let length = take(bytes, 2)?;
            let length = u16::from_be_bytes([length[0], length[1]]) as usize;
            let value = take(bytes, length)?;
            if kind == 6 {
                Ok(None)
            } else {
                std::str::from_utf8(value)
                    .map(Some)
                    .map_err(|_| error("string header value is not UTF-8"))
            }
        }
        9 => {
            take(bytes, 16)?;
            Ok(None)
        }
        _ => Err(error(format!("unknown header value type {kind}"))),
    }
}

fn take<'a>(bytes: &mut &'a [u8], length: usize) -> Result<&'a [u8], ChannelError> {
    if bytes.len() < length {
        return Err(error("header block is truncated"));
    }
    let (head, tail) = bytes.split_at(length);
    *bytes = tail;
    Ok(head)
}

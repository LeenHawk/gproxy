//! Auth-code callback parsing without retaining or logging authorization data.

pub(super) fn parse_callback(callback_url: &str) -> Option<(String, String)> {
    let uri: http::Uri = callback_url.parse().ok()?;
    let query = uri.query()?;
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        match key {
            "code" => code = Some(pct_decode(value)),
            "state" => state = Some(pct_decode(value)),
            _ => {}
        }
    }
    Some((code?, state?))
}

fn pct_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => output.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                if let (Some(high), Some(low)) = (high, low) {
                    output.push((high * 16 + low) as u8);
                    index += 3;
                    continue;
                }
                output.push(b'%');
            }
            byte => output.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

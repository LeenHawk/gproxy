pub(super) fn split<'a>(body: &'a [u8], boundary: &str) -> Vec<&'a [u8]> {
    let marker = format!("--{boundary}");
    let marker = marker.as_bytes();
    let mut parts = Vec::new();
    let Some(mut current) = find_boundary(body, marker, 0) else {
        return parts;
    };
    loop {
        let after = current + marker.len();
        if body.get(after..after + 2) == Some(b"--") {
            break;
        }
        let Some(start) = body
            .get(after..after + 2)
            .filter(|bytes| *bytes == b"\r\n")
            .map(|_| after + 2)
        else {
            break;
        };
        let Some(next) = find_boundary(body, marker, start) else {
            break;
        };
        let end = next;
        if end >= start {
            parts.push(&body[start..end]);
        }
        current = next;
    }
    parts
}

fn find_boundary(body: &[u8], marker: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    while let Some(offset) = body
        .get(cursor..)?
        .windows(marker.len())
        .position(|window| window == marker)
    {
        let candidate = cursor + offset;
        let line_start =
            candidate == 0 || body.get(candidate.saturating_sub(2)..candidate) == Some(b"\r\n");
        let after = candidate + marker.len();
        let suffix = body.get(after..after + 2);
        if line_start && matches!(suffix, Some(b"\r\n") | Some(b"--")) {
            return Some(candidate);
        }
        cursor = candidate + 1;
    }
    None
}

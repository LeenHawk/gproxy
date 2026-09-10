use gproxy_channel_api::ChannelError;
use rust_decimal::Decimal;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Token<'a> {
    Word(&'a str),
    Number(&'a str),
    String(&'a str),
    P(u8),
}

pub(super) struct Billing {
    pub balance: Decimal,
    pub limit: Option<Decimal>,
    pub usage: Option<Decimal>,
    pub updated: Option<i64>,
}

pub(super) fn read(html: &str) -> Result<Billing, ChannelError> {
    let tokens = scripts(html).ok_or_else(super::invalid)?;
    let mut promise = None;
    for (index, token) in tokens.iter().enumerate() {
        if *token != Token::Word("_$HY")
            || !matches_at(
                &tokens,
                index + 1,
                &[Token::P(b'.'), Token::Word("r"), Token::P(b'[')],
            )
        {
            continue;
        }
        let Some(Token::String(raw)) = tokens.get(index + 4) else {
            continue;
        };
        let Some(key) = string(raw) else {
            continue;
        };
        let Some(args) = key.strip_prefix("billing.get") else {
            continue;
        };
        let workspace: Vec<String> = serde_json::from_str(args).map_err(|_| super::invalid())?;
        if workspace.len() != 1 || workspace[0].is_empty() || promise.is_some() {
            return Err(super::invalid());
        }
        let mut at = index + 5;
        expect(&tokens, &mut at, Token::P(b']'))?;
        expect(&tokens, &mut at, Token::P(b'='))?;
        reference(&tokens, &mut at).ok_or_else(super::invalid)?;
        expect(&tokens, &mut at, Token::P(b'='))?;
        reference(&tokens, &mut at).ok_or_else(super::invalid)?;
        expect(&tokens, &mut at, Token::P(b'('))?;
        let slot = reference(&tokens, &mut at).ok_or_else(super::invalid)?;
        expect(&tokens, &mut at, Token::P(b'='))?;
        expect(&tokens, &mut at, Token::P(b'{'))?;
        promise = Some((slot, at));
    }
    let (slot, anchor_end) = promise.ok_or_else(super::invalid)?;
    let mut resolved = None;
    for index in anchor_end..tokens.len() {
        let mut at = index;
        if reference(&tokens, &mut at).is_none() || tokens.get(at) != Some(&Token::P(b'(')) {
            continue;
        }
        at += 1;
        if reference(&tokens, &mut at) != Some(slot) || tokens.get(at) != Some(&Token::P(b',')) {
            continue;
        }
        at += 1;
        if reference(&tokens, &mut at).is_some() {
            expect(&tokens, &mut at, Token::P(b'='))?;
        }
        expect(&tokens, &mut at, Token::P(b'{'))?;
        if resolved.is_some() {
            return Err(super::invalid());
        }
        resolved = Some(object(&tokens, at)?);
    }
    let fields = resolved.ok_or_else(super::invalid)?;
    let required = |key| {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| *value)
            .ok_or_else(super::invalid)
    };
    let balance = amount(required("balance")?)?.ok_or_else(super::invalid)?;
    let limit = amount(required("monthlyLimit")?)?;
    if limit.is_some_and(|value| value < Decimal::ZERO) {
        return Err(super::invalid());
    }
    Ok(Billing {
        balance,
        limit,
        usage: amount(required("monthlyUsage")?)?,
        updated: date(required("timeMonthlyUsageUpdated")?, &tokens)?,
    })
}

fn scripts(html: &str) -> Option<Vec<Token<'_>>> {
    if html.len() > 8 * 1024 * 1024 {
        return None;
    }
    let mut tokens = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        rest = &rest[start + 7..];
        let opening_end = rest.find('>')?;
        rest = &rest[opening_end + 1..];
        let end = rest.find("</script>")?;
        tokenize(&rest[..end], &mut tokens)?;
        rest = &rest[end + 9..];
    }
    Some(tokens)
}

fn tokenize<'a>(script: &'a str, tokens: &mut Vec<Token<'a>>) -> Option<()> {
    let bytes = script.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        match bytes[at] {
            byte if byte.is_ascii_whitespace() => at += 1,
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at += 2;
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at += 2;
                at += script[at..].find("*/")? + 2;
            }
            b'"' | b'\'' | b'`' => {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    if bytes[at] == b'\\' {
                        at += 1;
                    }
                    at += 1;
                }
                if at >= bytes.len() {
                    return None;
                }
                at += 1;
                tokens.push(Token::String(&script[start..at]));
            }
            byte if byte.is_ascii_digit() => {
                at += 1;
                while at < bytes.len()
                    && (bytes[at].is_ascii_digit()
                        || matches!(bytes[at], b'.' | b'e' | b'E' | b'+' | b'-'))
                {
                    at += 1;
                }
                tokens.push(Token::Number(&script[start..at]));
            }
            byte if byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$') => {
                at += 1;
                while at < bytes.len()
                    && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'_' | b'$'))
                {
                    at += 1;
                }
                tokens.push(Token::Word(&script[start..at]));
            }
            byte if byte.is_ascii() => {
                at += 1;
                tokens.push(Token::P(byte));
            }
            _ => at += script[at..].chars().next()?.len_utf8(),
        }
    }
    Some(())
}

fn string(raw: &str) -> Option<String> {
    if raw.starts_with('"') {
        return serde_json::from_str(raw).ok();
    }
    let value = raw.strip_prefix('\'')?.strip_suffix('\'')?;
    if value.contains('\\') {
        return None;
    }
    Some(value.into())
}

fn matches_at(tokens: &[Token<'_>], at: usize, expected: &[Token<'_>]) -> bool {
    tokens.get(at..at + expected.len()) == Some(expected)
}

fn expect(tokens: &[Token<'_>], at: &mut usize, expected: Token<'_>) -> Result<(), ChannelError> {
    if tokens.get(*at) != Some(&expected) {
        return Err(super::invalid());
    }
    *at += 1;
    Ok(())
}

fn reference<'a>(tokens: &[Token<'a>], at: &mut usize) -> Option<&'a str> {
    if !matches_at(tokens, *at, &[Token::Word("$R"), Token::P(b'[')]) {
        return None;
    }
    let Some(Token::Number(slot)) = tokens.get(*at + 2) else {
        return None;
    };
    if !slot.bytes().all(|byte| byte.is_ascii_digit())
        || tokens.get(*at + 3) != Some(&Token::P(b']'))
    {
        return None;
    }
    *at += 4;
    Some(slot)
}

fn object<'a, 't>(
    tokens: &'t [Token<'a>],
    mut at: usize,
) -> Result<Vec<(String, &'t [Token<'a>])>, ChannelError> {
    let mut fields = Vec::new();
    while tokens.get(at) != Some(&Token::P(b'}')) {
        let name = match tokens.get(at) {
            Some(Token::Word(name)) => (*name).to_owned(),
            Some(Token::String(raw)) => string(raw).ok_or_else(super::invalid)?,
            _ => return Err(super::invalid()),
        };
        at += 1;
        expect(tokens, &mut at, Token::P(b':'))?;
        let start = at;
        let mut stack = Vec::new();
        while let Some(token) = tokens.get(at) {
            match token {
                Token::P(b',' | b'}') if stack.is_empty() => break,
                Token::P(b'{' | b'[' | b'(') => stack.push(*token),
                Token::P(close @ (b'}' | b']' | b')')) => {
                    let open = match close {
                        b'}' => b'{',
                        b']' => b'[',
                        _ => b'(',
                    };
                    if stack.pop() != Some(Token::P(open)) {
                        return Err(super::invalid());
                    }
                }
                _ => {}
            }
            at += 1;
        }
        if at == start || at >= tokens.len() || !stack.is_empty() {
            return Err(super::invalid());
        }
        if fields.iter().any(|(key, _)| key == &name) {
            return Err(super::invalid());
        }
        fields.push((name, &tokens[start..at]));
        if tokens.get(at) == Some(&Token::P(b',')) {
            at += 1;
        } else {
            break;
        }
    }
    if tokens.get(at) != Some(&Token::P(b'}')) || tokens.get(at + 1) != Some(&Token::P(b')')) {
        return Err(super::invalid());
    }
    Ok(fields)
}

fn amount(tokens: &[Token<'_>]) -> Result<Option<Decimal>, ChannelError> {
    let value = match tokens {
        [Token::Word("null")] => return Ok(None),
        [Token::Number(value)] => value.parse::<Decimal>().ok(),
        [Token::P(b'-'), Token::Number(value)] => value.parse::<Decimal>().ok().map(|value| -value),
        _ => None,
    };
    value.map(Some).ok_or_else(super::invalid)
}

fn date(tokens: &[Token<'_>], all: &[Token<'_>]) -> Result<Option<i64>, ChannelError> {
    if tokens == [Token::Word("null")] {
        return Ok(None);
    }
    let mut at = 0;
    let tokens = if let Some(slot) = reference(tokens, &mut at) {
        if tokens.get(at) == Some(&Token::P(b'=')) {
            &tokens[at + 1..]
        } else if at == tokens.len() {
            let mut value = None;
            for index in 0..all.len() {
                let mut cursor = index;
                if reference(all, &mut cursor) == Some(slot)
                    && all.get(cursor) == Some(&Token::P(b'='))
                {
                    if value.is_some() {
                        return Err(super::invalid());
                    }
                    let start = cursor + 1;
                    if !matches_at(
                        all,
                        start,
                        &[Token::Word("new"), Token::Word("Date"), Token::P(b'(')],
                    ) {
                        return Err(super::invalid());
                    }
                    value = Some(all.get(start..start + 5).ok_or_else(super::invalid)?);
                }
            }
            value.ok_or_else(super::invalid)?
        } else {
            return Err(super::invalid());
        }
    } else {
        tokens
    };
    let literal = match tokens {
        [
            Token::Word("new"),
            Token::Word("Date"),
            Token::P(b'('),
            literal,
            Token::P(b')'),
        ] => literal,
        [literal @ Token::String(_)] => literal,
        _ => return Err(super::invalid()),
    };
    match literal {
        Token::String(raw) => string(raw)
            .and_then(|value| super::super::super::quota::iso_to_unix(&value))
            .map(Some)
            .ok_or_else(super::invalid),
        Token::Number(raw) => raw
            .parse::<i64>()
            .ok()
            .map(|ms| Some(ms.div_euclid(1000)))
            .ok_or_else(super::invalid),
        _ => Err(super::invalid()),
    }
}

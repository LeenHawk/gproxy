use http::request::Parts;

use crate::AdminError;

pub(crate) fn verify_same_origin(parts: &Parts) -> Result<(), AdminError> {
    let Some(origin) = parts.headers.get(http::header::ORIGIN) else {
        return Ok(());
    };
    let origin = origin.to_str().map_err(|_| AdminError::Forbidden)?;
    let origin = origin
        .parse::<http::Uri>()
        .map_err(|_| AdminError::Forbidden)?;
    let origin = origin.authority().ok_or(AdminError::Forbidden)?.as_str();
    let host = parts
        .headers
        .get(http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or(AdminError::Forbidden)?;
    if origin.eq_ignore_ascii_case(host) {
        Ok(())
    } else {
        Err(AdminError::Forbidden)
    }
}

//! Serve explicitly installed fonts from the application data directory.

use std::sync::Arc;

use gproxy_host_axum::console::Console;
use http::{Response, StatusCode, header};
use tauri::Manager;

pub(crate) struct FontConsole(pub Arc<Console>);

pub(crate) fn register<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(
        "gproxy-fonts",
        |context, request, responder| {
            let console = context.app_handle().state::<FontConsole>().0.clone();
            tauri::async_runtime::spawn(async move {
                let response = if request.uri().path().starts_with("/console/fonts/") {
                    console.serve(request.method(), request.uri().path()).await
                } else {
                    None
                };
                let mut response = match response {
                    Some(response) => {
                        let (parts, body) = response.into_parts();
                        match axum::body::to_bytes(body, usize::MAX).await {
                            Ok(bytes) => Response::from_parts(parts, bytes.to_vec()),
                            Err(_) => Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Vec::new())
                                .unwrap(),
                        }
                    }
                    None => Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Vec::new())
                        .unwrap(),
                };
                response.headers_mut().insert(
                    header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    header::HeaderValue::from_static("*"),
                );
                responder.respond(response);
            });
        },
    )
}

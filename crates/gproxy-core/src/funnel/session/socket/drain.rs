use futures_util::future::{Either, select};
use gproxy_channel_api::WsFrame;

use super::{Host, Session, Shared};
use crate::usage::Ended;

pub(super) async fn finish<H: Host>(host: Shared<H>, mut session: Session<H>, mut ended: Ended) {
    let started = web_time::Instant::now();
    loop {
        let timeout = std::time::Duration::from_secs(3).saturating_sub(started.elapsed());
        if timeout.is_zero() {
            ended = Ended::Interrupted;
            break;
        }
        let received = {
            let receive = session.socket.recv();
            let deadline = host.wait(timeout);
            match select(receive, deadline).await {
                Either::Left((result, _)) => Some(result),
                Either::Right(_) => None,
            }
        };
        match received {
            Some(Ok(Some(WsFrame::Close(code)))) => {
                if code.is_some_and(|code| code != 1000) {
                    ended = Ended::Interrupted;
                }
                break;
            }
            Some(Ok(Some(frame))) => {
                if session.observe(&frame).is_err() {
                    ended = Ended::Interrupted;
                    break;
                }
            }
            Some(Ok(None)) => break,
            Some(Err(_)) | None => {
                ended = Ended::Interrupted;
                break;
            }
        }
    }
    session.guard.finish(ended).await;
}

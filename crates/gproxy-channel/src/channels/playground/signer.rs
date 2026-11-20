//! Live WAA signer sessions, one per Google session and model.
//!
//! Building a [`waa::Session`] (page realms, interpreter, `initialize`) costs a
//! Waa/Create round trip and a second or two of CPU, while each further proof from
//! the same VM is cheap. A session is `!Send`, so each one lives on its own thread
//! and takes jobs over a channel; the pool keeps those threads between requests.
//!
//! A worker is replaced by a fresh Waa/Create when it is older than
//! [`SESSION_TTL`], when its thread has ended, when a proof fails, or when Google
//! rejects a request signed by it. Dropping a `Worker` closes its job channel; the
//! thread then leaves its loop and drops the session there, which releases the JS
//! roots before the QuickJS runtime is freed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

use super::{
    auth::Session,
    request::{self, invalid},
    waa::{self, WaaError},
};
use crate::channel::ChannelError;

/// How long one Waa/Create program and its snapshot function are reused. The
/// reference signer keeps a VM for twelve hours; this is deliberately short.
const SESSION_TTL: Duration = Duration::from_secs(5 * 60);

struct Job {
    binding: String,
    page: String,
    reply: oneshot::Sender<Result<String, WaaError>>,
}

struct Worker {
    jobs: mpsc::Sender<Job>,
    thread: JoinHandle<()>,
    built: Instant,
}

impl Worker {
    /// Start a thread, build the session on it and wait until it is ready.
    async fn start(input: waa::SessionInput) -> Result<Self, WaaError> {
        let (jobs, queue) = mpsc::channel::<Job>();
        let (ready_tx, ready) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("gproxy-waa".into())
            .stack_size(8 << 20)
            .spawn(move || {
                let session = match waa::Session::new(input) {
                    Ok(session) => session,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                while let Ok(job) = queue.recv() {
                    let result = session.prove(&job.binding, &job.page);
                    let failed = result.is_err();
                    let _ = job.reply.send(result);
                    if failed {
                        break;
                    }
                }
                // `session` drops here, on the thread that owns the runtime.
            })
            .map_err(|e| WaaError(format!("spawn signer thread: {e}")))?;
        ready
            .await
            .map_err(|_| WaaError("signer thread ended while starting".into()))??;
        Ok(Self {
            jobs,
            thread,
            built: Instant::now(),
        })
    }

    async fn prove(&self, binding: String, page: String) -> Result<String, WaaError> {
        let (reply, result) = oneshot::channel();
        self.jobs
            .send(Job {
                binding,
                page,
                reply,
            })
            .map_err(|_| WaaError("signer thread has ended".into()))?;
        result
            .await
            .map_err(|_| WaaError("signer thread ended during a proof".into()))?
    }

    fn stale(&self) -> bool {
        self.built.elapsed() >= SESSION_TTL || self.thread.is_finished()
    }
}

type Key = [u8; 32];
/// The async lock serializes one session's requests, so two of them never build
/// two workers; the worker would run them one after the other anyway.
type Slot = Arc<tokio::sync::Mutex<Option<Worker>>>;

fn pool() -> &'static Mutex<HashMap<Key, Slot>> {
    static POOL: OnceLock<Mutex<HashMap<Key, Slot>>> = OnceLock::new();
    POOL.get_or_init(Default::default)
}

/// Proofs are bound to the account, and the page profile carries the model.
fn key(session: &Session, model_short: &str) -> Key {
    let mut hasher = Sha256::new();
    hasher.update(session.identity().as_bytes());
    hasher.update([0]);
    hasher.update(model_short.as_bytes());
    hasher.finalize().into()
}

/// This key's slot. Also drops every other slot that is idle and holds no usable
/// worker, so the pool only keeps sessions used within the TTL.
fn slot(key: Key) -> Slot {
    let mut pool = pool().lock().unwrap_or_else(|e| e.into_inner());
    pool.retain(|other, slot| {
        *other == key
            || Arc::strong_count(slot) > 1
            || match slot.try_lock() {
                Ok(worker) => worker.as_ref().is_some_and(|worker| !worker.stale()),
                Err(_) => true,
            }
    });
    pool.entry(key).or_default().clone()
}

/// Mint the proof for one request, on this session's live worker when there is
/// one and on a newly built worker otherwise.
pub(super) async fn proof(
    client: &dyn gproxy_client::OutboundClient,
    session: &Session,
    tier: Option<&str>,
    model_short: &str,
    binding: String,
    page: String,
) -> Result<String, ChannelError> {
    let slot = slot(key(session, model_short));
    let mut held = slot.lock().await;
    if let Some(worker) = held.as_ref().filter(|worker| !worker.stale()) {
        if let Ok(proof) = worker.prove(binding.clone(), page.clone()).await {
            if proof.starts_with('!') {
                return Ok(proof);
            }
        }
    }
    // Stale, failed or absent: end the old worker and start over from Waa/Create.
    *held = None;
    let challenge = request::fetch_create(client, session, tier).await?;
    if challenge.interpreter_url.is_empty() {
        return Err(invalid("Waa/Create returned no interpreter URL"));
    }
    let interpreter = request::fetch_interpreter(client, &challenge.interpreter_url).await?;
    let profile = waa::build_top_profile(&session.cookie(), model_short)
        .map_err(|e| invalid(e.to_string()))?;
    let worker = Worker::start(waa::SessionInput {
        program: challenge.program,
        interpreter,
        source_name: challenge.interpreter_url,
        global_name: challenge.global_name,
        top_profile_json: profile,
    })
    .await
    .map_err(|e| invalid(e.to_string()))?;
    let proof = worker
        .prove(binding, page)
        .await
        .map_err(|e| invalid(e.to_string()))?;
    if proof.starts_with('!') {
        *held = Some(worker);
    }
    Ok(proof)
}

/// Forget this session's worker after Google rejected a request it signed. A
/// worker that another request is using right now is left to that request.
pub(super) fn discard(session: &Session, model_short: &str) {
    let key = key(session, model_short);
    let slot = pool()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned();
    if let Some(slot) = slot {
        if let Ok(mut held) = slot.try_lock() {
            *held = None;
        }
    }
}

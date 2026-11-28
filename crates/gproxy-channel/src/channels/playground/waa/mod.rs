//! AI Studio Playground WAA / BotGuard proof signer, hosted on QuickJS (rquickjs).
//!
//! Google AI Studio requires a per-request anti-bot "proof": the browser runs a
//! Google-delivered, periodically-rotated obfuscated interpreter inside a faithful
//! Firefox DOM environment, which emits a `!`-prefixed attestation string. We
//! re-host that same environment (`dom.js` + `smhost.js` + `hostjs`) on QuickJS and
//! run the real interpreter verbatim, so the signer is build-agnostic. Native only:
//! QuickJS is C and cannot target `wasm32-unknown-unknown`, so the playground channel
//! mints in-process on native and is unavailable on the edge/Worker build.
//!
//! A [`Session`] owns one initialized VM (realms + interpreter + the `snapshot`
//! function `initialize` hands back) and mints any number of proofs from it with
//! [`Session::prove`]. A session is `!Send` and lives on the thread that built it,
//! one session per thread.
//!
//! Values are RAII `Value/Object/Function`, so the `Runtime` drops cleanly. Native
//! functions are plain generic `fn`s (not closures) reading a thread-local pointer to
//! the session's `Host`; this avoids both the capture/HRTB friction of returning
//! `'js`-invariant values from closures and the native→`Rc<Host>`→JS reference cycle
//! that the GC cannot trace. `Host`'s rooted `Persistent` handles are cleared by
//! `Session`'s `Drop` before teardown so no JS value leaks into `JS_FreeRuntime`'s
//! assert.

use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Instant;

use rquickjs::function::Opt;
use rquickjs::{qjs, Array, Context, Ctx, Function, Object, Persistent, Runtime, Value};

const DOM_JS: &str = include_str!("assets/dom.js");
const SMHOST_JS: &str = include_str!("assets/smhost.js");
const HOST_JS: &str = include_str!("assets/hostjs.js");
const HELPER_JS: &str = include_str!("assets/helper.js");
const PROMPT_JS: &str = include_str!("assets/prompt.js");
const NOOP_JS: &str = include_str!("assets/noop.js");
/// Empties the prompt box after a proof, as the page does once a prompt is sent.
const CLEAR_PROMPT_JS: &str = "(() => { const box = document.querySelector('ms-prompt-box'); if (box) Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box.querySelector('textarea'), ''); })";
const BOOTSTRAP: &str = include_str!("assets/bootstrap.js");
const SHAPE_DEFLATE: &[u8] = include_bytes!("assets/shape.json.deflate");
const PROFILE_TEMPLATE: &str = include_str!("assets/profile.template.json");

/// The Firefox release the WAA environment and proof attest. wreq-util emulates
/// up to 151; this UA (152) is what the embedded profile reports, and is sent as
/// the `User-Agent` header so the request matches the proof.
pub const FIREFOX_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:152.0) Gecko/20100101 Firefox/152.0";
pub const WAA_CREATE_URL: &str = "https://waa-pa.clients6.google.com/$rpc/google.internal.waa.v1.Waa/Create";
pub const WAA_API_KEY: &str = "AIzaSyBGb5fGAyC-pRcRU6MUHb__b_vKha71HRE";
pub const WAA_REQUEST_KEY: &str = "lmnUSbltwc5ULv48iKLX";

/// What a [`Session`] is built from: one Waa/Create challenge, the interpreter it
/// names, and the page profile.
pub struct SessionInput {
    pub program: String,
    pub interpreter: String,
    pub source_name: String,
    pub global_name: String,
    pub top_profile_json: String,
}

#[derive(Debug)]
pub struct WaaError(pub String);
impl std::fmt::Display for WaaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WAA signer: {}", self.0)
    }
}
impl std::error::Error for WaaError {}
fn err<T>(m: impl Into<String>) -> Result<T, WaaError> {
    Err(WaaError(m.into()))
}

struct Timer {
    fire_at: f64,
    id: f64,
    realm: usize,
    arg: Option<bool>,
}

/// Page functions a session calls for every proof.
#[derive(Clone)]
struct ProveFns {
    prompt: Persistent<Function<'static>>,
    clear: Persistent<Function<'static>>,
    bind: Persistent<Function<'static>>,
    options: Persistent<Function<'static>>,
    deliver: Persistent<Function<'static>>,
}

struct Host {
    real_start: Instant,
    virtual_add: Cell<f64>,
    timers: RefCell<Vec<Timer>>,
    realm_ctxs: RefCell<Vec<*mut qjs::JSContext>>,
    frame_ctxs: RefCell<Vec<*mut qjs::JSContext>>,
    dispatch: RefCell<Vec<Persistent<Function<'static>>>>,
    order_global: RefCell<Option<Persistent<Function<'static>>>>,
    shared: RefCell<Option<Persistent<Object<'static>>>>,
    snapshot: RefCell<Option<Persistent<Function<'static>>>>,
    prove_fns: RefCell<Option<ProveFns>>,
    /// Id of the latest snapshot request; a delivery under another id is stale.
    proof_id: Cell<f64>,
    proof: RefCell<Option<String>>,
    shape_json: String,
    top_profile: String,
    frame_profile: String,
    fresh_keys_json: String,
    source_name: String,
}

impl Host {
    fn now(&self) -> f64 {
        self.real_start.elapsed().as_secs_f64() * 1000.0 + self.virtual_add.get()
    }
    fn clear_persistents(&self) {
        self.dispatch.borrow_mut().clear();
        *self.order_global.borrow_mut() = None;
        *self.shared.borrow_mut() = None;
        *self.snapshot.borrow_mut() = None;
        *self.prove_fns.borrow_mut() = None;
    }
}

thread_local! {
    static CURRENT_HOST: Cell<*const Host> = const { Cell::new(std::ptr::null()) };
}
fn host<'a>() -> &'a Host {
    let p = CURRENT_HOST.with(|c| c.get());
    unsafe { &*p }
}

fn find_realm(ctx: &Ctx<'_>) -> usize {
    let p = ctx.as_raw().as_ptr();
    host()
        .realm_ctxs
        .borrow()
        .iter()
        .position(|&c| c == p)
        .unwrap_or(0)
}

fn unix_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    if n > 0 {
        getrandom::fill(&mut buf).expect("getrandom");
    }
    buf
}

fn inflate_shape() -> Result<String, WaaError> {
    let bytes = miniz_oxide::inflate::decompress_to_vec(SHAPE_DEFLATE)
        .map_err(|_| WaaError("shape inflate failed".into()))?;
    String::from_utf8(bytes).map_err(|_| WaaError("shape not utf-8".into()))
}

fn build_frame_profile(top: &serde_json::Value) -> String {
    const FRAME_WINDOW_NAMES: &[&str] = &[
        "outerWidth", "outerHeight", "screenX", "screenY", "screenLeft", "screenTop",
        "devicePixelRatio", "mozInnerScreenX",
    ];
    let mut w = serde_json::Map::new();
    if let Some(win) = top.get("window").and_then(|v| v.as_object()) {
        for n in FRAME_WINDOW_NAMES {
            if let Some(v) = win.get(*n) {
                w.insert((*n).to_string(), v.clone());
            }
        }
    }
    let mut values = serde_json::Map::new();
    if let Some(tv) = top.get("values").and_then(|v| v.as_object()) {
        if let Some(nav) = tv.get("navigator") {
            values.insert("navigator".into(), nav.clone());
        }
        if let Some(scr) = tv.get("screen") {
            values.insert("screen".into(), scr.clone());
        }
        if let Some(doc) = tv.get("document").and_then(|v| v.as_object()) {
            let mut d = serde_json::Map::new();
            for k in ["cookie", "lastModified"] {
                if let Some(v) = doc.get(k) {
                    d.insert(k.to_string(), v.clone());
                }
            }
            if !d.is_empty() {
                values.insert("document".into(), serde_json::Value::Object(d));
            }
        }
    }
    let mut result = serde_json::Map::new();
    result.insert(
        "hasFocus".into(),
        serde_json::Value::Bool(top.get("hasFocus").and_then(|v| v.as_bool()).unwrap_or(false)),
    );
    result.insert("window".into(), serde_json::Value::Object(w));
    result.insert("values".into(), serde_json::Value::Object(values));
    if let Some(ls) = top.get("localStorage") {
        result.insert("localStorage".into(), ls.clone());
    }
    serde_json::Value::Object(result).to_string()
}

// ---- unsafe point 1: sloppy-mode eval with a real source name ----
fn eval_named<'js>(ctx: &Ctx<'js>, code: &str, filename: &str) -> Result<Value<'js>, WaaError> {
    let cfile = CString::new(filename).map_err(|_| WaaError("bad source name".into()))?;
    let ccode = CString::new(code).map_err(|_| WaaError("source has NUL".into()))?;
    let raw = unsafe {
        qjs::JS_Eval(
            ctx.as_raw().as_ptr(),
            ccode.as_ptr(),
            code.len() as _,
            cfile.as_ptr(),
            qjs::JS_EVAL_TYPE_GLOBAL as i32,
        )
    };
    if unsafe { qjs::JS_IsException(raw) } {
        drop(unsafe { Value::from_raw(ctx.clone(), raw) });
        return Err(catch_text(ctx, filename));
    }
    Ok(unsafe { Value::from_raw(ctx.clone(), raw) })
}

fn catch_text(ctx: &Ctx<'_>, what: &str) -> WaaError {
    let e = ctx.catch();
    let msg = if let Some(ex) = e.as_exception() {
        format!("{}", ex)
    } else {
        e.as_string().and_then(|s| s.to_string().ok()).unwrap_or_else(|| "unknown".into())
    };
    WaaError(format!("{what}: {msg}"))
}

// ---- unsafe point 2: create an iframe realm context from inside a native ----
unsafe fn new_frame_ctx<'js>(ctx: &Ctx<'js>) -> Ctx<'js> {
    unsafe {
        let rt = qjs::JS_GetRuntime(ctx.as_raw().as_ptr());
        let raw = qjs::JS_NewContext(rt);
        host().frame_ctxs.borrow_mut().push(raw);
        Ctx::from_raw(NonNull::new(raw).expect("JS_NewContext"))
    }
}

// ---- native functions (plain generic fns reading the thread-local Host) ----
fn n_now() -> f64 {
    host().now()
}
fn n_schedule(ctx: Ctx<'_>, d: f64, id: f64) {
    let realm = find_realm(&ctx);
    host().timers.borrow_mut().push(Timer { fire_at: host().now() + d.max(0.0), id, realm, arg: None });
}
fn n_load_image(ctx: Ctx<'_>, _addr: String, id: f64) {
    let realm = find_realm(&ctx);
    host().timers.borrow_mut().push(Timer { fire_at: host().now() + 1.0, id, realm, arg: Some(true) });
}
fn n_random_uuid() -> String {
    let mut b = random_bytes(16);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0],b[1],b[2],b[3],b[4],b[5],b[6],b[7],b[8],b[9],b[10],b[11],b[12],b[13],b[14],b[15]
    )
}
fn n_random_bytes<'js>(ctx: Ctx<'js>, n: f64) -> rquickjs::Result<rquickjs::ArrayBuffer<'js>> {
    rquickjs::ArrayBuffer::new(ctx, random_bytes(n.max(0.0) as usize))
}
/// A copy of an ArrayBuffer's bytes; empty when the buffer is detached.
fn buffer_bytes(data: &rquickjs::ArrayBuffer<'_>) -> Vec<u8> {
    // SAFETY: the engine-owned slice is copied out before this returns, and no
    // JavaScript runs while it is borrowed.
    unsafe { data.as_bytes() }.map(<[u8]>::to_vec).unwrap_or_default()
}
fn n_digest<'js>(ctx: Ctx<'js>, name: String, data: rquickjs::ArrayBuffer<'js>) -> rquickjs::Result<rquickjs::ArrayBuffer<'js>> {
    use sha1::Sha1;
    use sha2::{Digest, Sha256, Sha384, Sha512};
    let bytes = buffer_bytes(&data);
    let bytes = bytes.as_slice();
    let out: Vec<u8> = match name.as_str() {
        "SHA-1" => Sha1::digest(bytes).to_vec(),
        "SHA-384" => Sha384::digest(bytes).to_vec(),
        "SHA-512" => Sha512::digest(bytes).to_vec(),
        _ => Sha256::digest(bytes).to_vec(),
    };
    rquickjs::ArrayBuffer::new(ctx, out)
}
fn n_encoding_name(label: String) -> String {
    match label.trim().to_ascii_lowercase().as_str() {
        "" | "utf-8" | "utf8" | "unicode-1-1-utf-8" => "utf-8".to_string(),
        "utf-16le" | "utf-16" => "utf-16le".to_string(),
        "utf-16be" => "utf-16be".to_string(),
        "iso-8859-1" | "latin1" | "windows-1252" | "ascii" | "us-ascii" => "windows-1252".to_string(),
        _ => String::new(),
    }
}
fn n_decode_text<'js>(ctx: Ctx<'js>, name: String, data: rquickjs::ArrayBuffer<'js>, fatal: bool) -> rquickjs::Result<Value<'js>> {
    let n = name.to_ascii_lowercase();
    let bytes = buffer_bytes(&data);
    let s: Option<String> = if n.contains("16le") || n == "utf-16" {
        let u: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16(&u).ok()
    } else if n.contains("1252") || n.contains("8859") || n.contains("latin") {
        Some(bytes.iter().map(|&b| b as char).collect())
    } else if fatal {
        String::from_utf8(bytes).ok()
    } else {
        Some(String::from_utf8_lossy(&bytes).into_owned())
    };
    match s {
        Some(txt) => Ok(rquickjs::String::from_str(ctx, &txt)?.into_value()),
        None => Err(ctx.throw(rquickjs::String::from_str(ctx.clone(), "TextDecoder.decode: Decoding failed.")?.into_value())),
    }
}
fn url_record(u: &url::Url) -> String {
    let proto = format!("{}:", u.scheme());
    let hostname = u.host_str().unwrap_or("").to_string();
    let port = u.port().map(|p| p.to_string()).unwrap_or_default();
    let host = if port.is_empty() { hostname.clone() } else { format!("{hostname}:{port}") };
    let origin = match u.scheme() {
        "http" | "https" | "ws" | "wss" | "ftp" => format!("{proto}//{host}"),
        _ => "null".to_string(),
    };
    let search = u.query().map(|q| format!("?{q}")).unwrap_or_default();
    let hash = u.fragment().map(|f| format!("#{f}")).unwrap_or_default();
    serde_json::json!({
        "href": u.as_str(), "origin": origin, "protocol": proto,
        "username": u.username(), "password": u.password().unwrap_or(""),
        "host": host, "hostname": hostname, "port": port,
        "pathname": u.path(), "search": search, "hash": hash
    })
    .to_string()
}
fn n_parse_url<'js>(ctx: Ctx<'js>, t: String, base: Opt<String>) -> rquickjs::Result<Value<'js>> {
    let parsed = match base.0 {
        Some(b) => url::Url::options().base_url(url::Url::parse(&b).ok().as_ref()).parse(&t),
        None => url::Url::parse(&t),
    };
    match parsed {
        Ok(u) => Ok(rquickjs::String::from_str(ctx, &url_record(&u))?.into_value()),
        Err(_) => Ok(Value::new_null(ctx)),
    }
}
fn n_update_url<'js>(ctx: Ctx<'js>, href: String, field: String, value: String) -> rquickjs::Result<Value<'js>> {
    let mut u = match url::Url::parse(&href) {
        Ok(u) => u,
        Err(_) => return Ok(Value::new_null(ctx)),
    };
    match field.as_str() {
        "href" => match url::Url::parse(&value) {
            Ok(n) => return Ok(rquickjs::String::from_str(ctx, &url_record(&n))?.into_value()),
            Err(_) => return Ok(Value::new_null(ctx)),
        },
        "protocol" => { let _ = u.set_scheme(value.trim_end_matches(':')); }
        "username" => { let _ = u.set_username(&value); }
        "password" => { let _ = u.set_password(if value.is_empty() { None } else { Some(&value) }); }
        "host" | "hostname" => { let _ = u.set_host(Some(&value)); }
        "port" => { let _ = u.set_port(value.parse().ok()); }
        "pathname" => u.set_path(&value),
        "search" => u.set_query(if value.is_empty() { None } else { Some(value.trim_start_matches('?')) }),
        "hash" => u.set_fragment(if value.is_empty() { None } else { Some(value.trim_start_matches('#')) }),
        _ => {}
    }
    Ok(rquickjs::String::from_str(ctx, &url_record(&u))?.into_value())
}
fn n_create_iframe<'js>(ctx: Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    let fctx = unsafe { new_frame_ctx(&ctx) };
    build_realm(&fctx, false)
        .map_err(|e| rquickjs::Error::new_from_js_message("waa", "frame-realm", e.0))?;
    Ok(fctx.globals())
}
fn n_cb_ready<'js>(ctx: Ctx<'js>, snap: Value<'js>, _shut: Opt<Value<'js>>) {
    if let Some(f) = snap.into_function() {
        *host().snapshot.borrow_mut() = Some(Persistent::save(&ctx, f));
    }
}
fn n_deliver(id: f64, val: Value<'_>) {
    if id != host().proof_id.get() {
        return;
    }
    let s = val
        .as_string()
        .and_then(|s| s.to_string().ok())
        .or_else(|| val.as_float().map(|n| n.to_string()))
        .unwrap_or_default();
    *host().proof.borrow_mut() = Some(s);
}

fn build_realm(ctx: &Ctx<'_>, is_top: bool) -> Result<(), WaaError> {
    let h = host();
    let idx = h.realm_ctxs.borrow().len();
    h.realm_ctxs.borrow_mut().push(ctx.as_raw().as_ptr());

    let globals = ctx.globals();
    let hobj = Object::new(ctx.clone()).map_err(|e| WaaError(format!("host obj: {e}")))?;
    macro_rules! native {
        ($name:literal, $f:expr) => {{
            let f = Function::new(ctx.clone(), $f)
                .map_err(|e| WaaError(format!("native {}: {e}", $name)))?
                .with_name($name)
                .map_err(|e| WaaError(format!("{e}")))?;
            hobj.set($name, f).map_err(|e| WaaError(format!("set {}: {e}", $name)))?;
        }};
    }
    native!("now", n_now);
    native!("schedule", n_schedule);
    native!("loadImage", n_load_image);
    native!("randomUUID", n_random_uuid);
    native!("randomBytes", n_random_bytes);
    native!("digest", n_digest);
    native!("encodingName", n_encoding_name);
    native!("decodeText", n_decode_text);
    native!("parseURL", n_parse_url);
    native!("updateURL", n_update_url);
    if is_top {
        native!("createIframeWindow", n_create_iframe);
    }
    globals.set("__host", hobj).map_err(|e| WaaError(format!("set __host: {e}")))?;

    let iframe_line = if is_top { "natives.createIframeWindow = () => H.createIframeWindow();" } else { "" };
    let boot = BOOTSTRAP.replace("__IFRAME_LINE__", iframe_line);
    eval_named(ctx, &boot, "waa-bootstrap")?;

    let install_dom = eval_named(ctx, DOM_JS, &format!("{}#dom", h.source_name))?;
    let install = eval_named(ctx, HOST_JS, "waa-hostjs")?;
    let build_host = eval_named(ctx, SMHOST_JS, "waa-smhost")?;

    let config = Object::new(ctx.clone()).map_err(|e| WaaError(format!("config: {e}")))?;
    config.set("realm", if is_top { "top" } else { "frame" }).ok();
    config.set("trustedTypes", false).ok();
    config.set("profileJSON", if is_top { h.top_profile.as_str() } else { h.frame_profile.as_str() }).ok();
    config.set("timeOrigin", unix_ms()).ok();
    config.set("started", h.now()).ok();
    if is_top {
        config.set("shapeJSON", h.shape_json.as_str()).ok();
    }

    let natives_obj: Value = globals.get("__natives").map_err(|e| WaaError(format!("get __natives: {e}")))?;
    let build_fn = build_host.into_function().ok_or_else(|| WaaError("buildHost not a function".into()))?;
    let shared_arg: Value = if is_top {
        Value::new_undefined(ctx.clone())
    } else {
        match &*h.shared.borrow() {
            Some(p) => p.clone().restore(ctx).map_err(|e| WaaError(format!("restore shared: {e}")))?.into_value(),
            None => Value::new_undefined(ctx.clone()),
        }
    };
    let result: Object = build_fn
        .call((natives_obj, install_dom, install, config, shared_arg))
        .map_err(|_| catch_text(ctx, "buildHost"))?;

    let dispatch: Function = result.get("dispatch").map_err(|e| WaaError(format!("get dispatch: {e}")))?;
    debug_assert_eq!(h.dispatch.borrow().len(), idx);
    h.dispatch.borrow_mut().push(Persistent::save(ctx, dispatch));
    if is_top {
        let order: Function = result.get("orderGlobal").map_err(|e| WaaError(format!("get orderGlobal: {e}")))?;
        *h.order_global.borrow_mut() = Some(Persistent::save(ctx, order));
        let shared: Object = result.get("shared").map_err(|e| WaaError(format!("get shared: {e}")))?;
        *h.shared.borrow_mut() = Some(Persistent::save(ctx, shared));
    } else {
        let code = format!(
            r#"(() => {{ const g = globalThis; const O = Object; const gopn = O.getOwnPropertyNames; const gopd = O.getOwnPropertyDescriptor; const defp = O.defineProperty; const want = {}; const wset = new Set(want);
  for (const k of gopn(g)) {{ if (!wset.has(k)) {{ const d = gopd(g, k); if (d && d.configurable && d.enumerable) {{ try {{ delete g[k]; }} catch {{}} }} }} }}
  for (const k of want) {{ if (k === 'undefined') continue; const d = gopd(g, k); if (!d || !d.configurable) continue; delete g[k]; defp(g, k, d); }}
}})();"#,
            h.fresh_keys_json
        );
        eval_named(ctx, &code, "waa-conform-frame")?;
    }
    Ok(())
}

fn flush_micro(ctx: &Ctx<'_>) {
    // `drain_jobs` stops at a job that throws; calling it again carries on with
    // the rest of the queue, the way a browser's microtask checkpoint does.
    for _ in 0..200_000 {
        if ctx.drain_jobs().is_ok() {
            break;
        }
    }
}

fn dispatch_timer(ctx: &Ctx<'_>, realm: usize, id: f64, arg: Option<bool>) -> Result<(), WaaError> {
    let disp = {
        let d = host().dispatch.borrow();
        match d.get(realm) {
            Some(p) => p.clone().restore(ctx).map_err(|e| WaaError(format!("restore dispatch: {e}")))?,
            None => return Ok(()),
        }
    };
    let arg_v: Value = match arg {
        Some(b) => Value::new_bool(ctx.clone(), b),
        None => Value::new_undefined(ctx.clone()),
    };
    disp.call::<_, Value>((id, arg_v)).map_err(|_| catch_text(ctx, "dispatch"))?;
    Ok(())
}

fn pump<F: Fn() -> bool>(
    ctx: &Ctx<'_>,
    advance: bool,
    pred: F,
    max_steps: u64,
    max_virtual_ms: f64,
) -> Result<(), WaaError> {
    flush_micro(ctx);
    let start_v = host().now();
    let mut n = 0u64;
    while !pred() {
        let now = host().now();
        let picked = {
            let mut timers = host().timers.borrow_mut();
            if advance {
                if timers.is_empty() {
                    None
                } else {
                    timers.sort_by(|a, b| a.fire_at.partial_cmp(&b.fire_at).unwrap());
                    let t = timers.remove(0);
                    Some((t.realm, t.id, t.arg, t.fire_at))
                }
            } else {
                match timers.iter().position(|t| t.fire_at <= now) {
                    Some(pos) => {
                        let t = timers.remove(pos);
                        Some((t.realm, t.id, t.arg, t.fire_at))
                    }
                    None => None,
                }
            }
        };
        let (realm, id, arg, fire_at) = match picked {
            Some(v) => v,
            None => break,
        };
        n += 1;
        if n > max_steps {
            return err("event-loop step budget exceeded");
        }
        if advance {
            if fire_at > host().now() {
                host().virtual_add.set(host().virtual_add.get() + (fire_at - host().now()));
            }
            if host().now() - start_v > max_virtual_ms {
                return err("event-loop virtual-time budget exceeded");
            }
        }
        dispatch_timer(ctx, realm, id, arg)?;
        flush_micro(ctx);
    }
    Ok(())
}

/// One initialized WAA VM: the QuickJS runtime, the page realms, the interpreter
/// and the `snapshot` function `initialize` produced. Building it is the expensive
/// part; each [`Session::prove`] afterwards only fills the prompt and snapshots.
///
/// Blocking and `!Send`: it stays on the thread that built it, which can hold one
/// session at a time (the natives find their `Host` through a thread-local).
pub struct Session {
    // Dropped in this order, after `Drop::drop` has released every JS root.
    host: Rc<Host>,
    ctx: Context,
    _rt: Runtime,
}

impl Session {
    pub fn new(input: SessionInput) -> Result<Self, WaaError> {
        if !CURRENT_HOST.with(|c| c.get()).is_null() {
            return err("a session is already live on this thread");
        }
        let shape_json = inflate_shape()?;
        let shape_val: serde_json::Value =
            serde_json::from_str(&shape_json).map_err(|_| WaaError("parse shape".into()))?;
        let fresh_keys_json = shape_val
            .get("frame")
            .and_then(|f| f.get("freshKeys"))
            .map(|v| v.to_string())
            .ok_or_else(|| WaaError("shape.frame.freshKeys missing".into()))?;
        let top_profile_val: serde_json::Value =
            serde_json::from_str(&input.top_profile_json).map_err(|_| WaaError("parse profile".into()))?;
        let frame_profile = build_frame_profile(&top_profile_val);

        let host = Rc::new(Host {
            real_start: Instant::now(),
            virtual_add: Cell::new(0.0),
            timers: RefCell::new(Vec::new()),
            realm_ctxs: RefCell::new(Vec::new()),
            frame_ctxs: RefCell::new(Vec::new()),
            dispatch: RefCell::new(Vec::new()),
            order_global: RefCell::new(None),
            shared: RefCell::new(None),
            snapshot: RefCell::new(None),
            prove_fns: RefCell::new(None),
            proof_id: Cell::new(0.0),
            proof: RefCell::new(None),
            shape_json,
            top_profile: input.top_profile_json.clone(),
            frame_profile,
            fresh_keys_json,
            source_name: input.source_name.clone(),
        });

        let rt = Runtime::new().map_err(|e| WaaError(format!("runtime: {e}")))?;
        rt.set_memory_limit(1 << 30);
        let ctx = Context::full(&rt).map_err(|e| WaaError(format!("context: {e}")))?;

        CURRENT_HOST.with(|c| c.set(Rc::as_ptr(&host)));
        // From here `Drop` owns the teardown, on the error path as well.
        let session = Session { host, ctx, _rt: rt };
        session.ctx.with(|ctx| initialize(&ctx, &input))?;
        Ok(session)
    }

    /// Mint one proof from the live VM. `binding` is the text the proof digest
    /// binds to; `page` is typed into the prompt box before snapshotting.
    pub fn prove(&self, binding: &str, page: &str) -> Result<String, WaaError> {
        let digest = sha256_hex(binding.as_bytes());
        self.ctx.with(|ctx| run_prove(&ctx, &digest, page))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Break any JS↔Host roots before teardown, then free the frame contexts.
        // The fields (context, then runtime) drop after this.
        self.host.clear_persistents();
        self.host.timers.borrow_mut().clear();
        for p in self.host.frame_ctxs.borrow_mut().drain(..) {
            // ---- unsafe point 3: free the iframe realm contexts ----
            unsafe { qjs::JS_FreeContext(p) };
        }
        CURRENT_HOST.with(|c| c.set(std::ptr::null()));
    }
}

/// Build the page, run the interpreter and `initialize`, and pump until the
/// `snapshot` function is ready.
fn initialize(ctx: &Ctx<'_>, input: &SessionInput) -> Result<(), WaaError> {
    build_realm(ctx, true)?;
    pump(ctx, false, || false, 20_000, 120_000.0).ok();

    let globals = ctx.globals();
    let helpers = eval_named(ctx, HELPER_JS, "waa-helper")?;
    let helpers = helpers.as_object().ok_or_else(|| WaaError("helpers".into()))?;
    let array_fn: Function = helpers.get("array").map_err(|_| WaaError("helpers.array".into()))?;
    let options_fn: Function = helpers.get("options").map_err(|_| WaaError("helpers.options".into()))?;
    let bind_fn: Function = helpers.get("bind").map_err(|_| WaaError("helpers.bind".into()))?;
    let prompt_fn = eval_named(ctx, PROMPT_JS, "waa-prompt")?.into_function().ok_or_else(|| WaaError("prompt fn".into()))?;
    let clear_fn = eval_named(ctx, CLEAR_PROMPT_JS, "waa-prompt")?.into_function().ok_or_else(|| WaaError("clear fn".into()))?;

    eval_named(ctx, &input.interpreter, &host().source_name)?;
    pump(ctx, false, || false, 20_000, 120_000.0).ok();

    if let Some(order) = host().order_global.borrow().clone() {
        let order = order.restore(ctx).map_err(|e| WaaError(format!("restore orderGlobal: {e}")))?;
        order.call::<_, Value>(()).map_err(|_| catch_text(ctx, "orderGlobal"))?;
        pump(ctx, false, || false, 20_000, 120_000.0).ok();
    }

    let botguard: Object = globals.get(input.global_name.as_str()).map_err(|_| WaaError("botguard undefined".into()))?;
    let initialize: Function = botguard.get("a").map_err(|_| WaaError("botguard.a".into()))?;
    let signals = eval_named(ctx, "[[],[]]", "waa-signals")?;
    let cb_ready = Function::new(ctx.clone(), n_cb_ready).map_err(|e| WaaError(format!("cb_ready: {e}")))?;
    let deliver = Function::new(ctx.clone(), n_deliver).map_err(|e| WaaError(format!("deliver: {e}")))?.with_name("deliver").map_err(|e| WaaError(format!("{e}")))?;

    let noop = eval_named(ctx, NOOP_JS, "waa-noop")?;
    let noop = noop.as_array().ok_or_else(|| WaaError("noop array".into()))?;
    let cb0: Value = noop.get(0).map_err(|_| WaaError("noop0".into()))?;
    let cb1: Value = noop.get(1).map_err(|_| WaaError("noop1".into()))?;
    let cb2: Value = noop.get(2).map_err(|_| WaaError("noop2".into()))?;
    let cb3: Value = noop.get(3).map_err(|_| WaaError("noop3".into()))?;
    let cb4: Value = noop.get(4).map_err(|_| WaaError("noop4".into()))?;
    let loggers: Array = array_fn.call((cb1, cb2, cb3, cb4)).map_err(|_| catch_text(ctx, "array(loggers)"))?;

    // initialize(program, cbReady, true, undefined, cb0, signals, undefined, false, loggers) — 9 args.
    let mut args = rquickjs::function::Args::new(ctx.clone(), 9);
    args.push_arg(input.program.as_str()).ok();
    args.push_arg(cb_ready).ok();
    args.push_arg(true).ok();
    args.push_arg(Value::new_undefined(ctx.clone())).ok();
    args.push_arg(cb0).ok();
    args.push_arg(signals).ok();
    args.push_arg(Value::new_undefined(ctx.clone())).ok();
    args.push_arg(false).ok();
    args.push_arg(loggers).ok();
    args.this(botguard).ok();
    args.apply::<Value>(&initialize).map_err(|_| catch_text(ctx, "initialize"))?;

    pump(ctx, true, || host().snapshot.borrow().is_some(), 200_000, 120_000.0)?;
    if host().snapshot.borrow().is_none() {
        return err("no snapshot produced");
    }
    *host().prove_fns.borrow_mut() = Some(ProveFns {
        prompt: Persistent::save(ctx, prompt_fn),
        clear: Persistent::save(ctx, clear_fn),
        bind: Persistent::save(ctx, bind_fn),
        options: Persistent::save(ctx, options_fn),
        deliver: Persistent::save(ctx, deliver),
    });
    Ok(())
}

/// `fillPrompt(page)` + `snapshot(bind(deliver, id), options(digest))`, pumped
/// until the proof is delivered.
fn run_prove(ctx: &Ctx<'_>, digest: &str, page: &str) -> Result<String, WaaError> {
    let h = host();
    let restore = |f: &Persistent<Function<'static>>, what: &str| {
        f.clone().restore(ctx).map_err(|e| WaaError(format!("restore {what}: {e}")))
    };
    let snapshot = match &*h.snapshot.borrow() {
        Some(p) => restore(p, "snapshot")?,
        None => return err("no snapshot produced"),
    };
    let fns = match h.prove_fns.borrow().clone() {
        Some(f) => f,
        None => return err("session not initialized"),
    };
    let prompt_fn = restore(&fns.prompt, "fillPrompt")?;
    let clear_fn = restore(&fns.clear, "clearPrompt")?;
    let bind_fn = restore(&fns.bind, "bind")?;
    let options_fn = restore(&fns.options, "options")?;
    let deliver = restore(&fns.deliver, "deliver")?;

    let id = h.proof_id.get() + 1.0;
    h.proof_id.set(id);
    *h.proof.borrow_mut() = None;

    prompt_fn.call::<_, Value>((page, false)).map_err(|_| catch_text(ctx, "fillPrompt"))?;
    pump(ctx, false, || false, 20_000, 120_000.0).ok();

    let bound_cb: Function = bind_fn.call((deliver, id)).map_err(|_| catch_text(ctx, "bind"))?;
    let opts: Value = options_fn.call((digest,)).map_err(|_| catch_text(ctx, "options"))?;
    snapshot.call::<_, Value>((bound_cb, opts)).map_err(|_| catch_text(ctx, "snapshot"))?;
    pump(ctx, true, || host().proof.borrow().is_some(), 200_000, 120_000.0)?;

    let proof = h.proof.borrow_mut().take();
    if clear_fn.call::<_, Value>(()).is_err() {
        let _ = ctx.catch();
    }
    match proof {
        Some(p) => Ok(p),
        None => err("no proof delivered"),
    }
}

// ---- Waa/Create challenge + profile + prompt helpers (used by the channel) ----

pub struct Challenge {
    pub program: String,
    pub global_name: String,
    pub interpreter_url: String,
}

fn jspb_str(v: &serde_json::Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}
fn jspb_first_str(v: &serde_json::Value) -> String {
    v.as_array()
        .and_then(|a| a.iter().find_map(|e| e.as_str().filter(|s| !s.is_empty())))
        .unwrap_or_default()
        .to_string()
}

/// Parse a Waa/Create response. Field 2 carries the obfuscated challenge
/// (base64, each byte +97, then a JSON array); field 1 is the plaintext form.
pub fn parse_challenge(raw: &[u8]) -> Result<Challenge, WaaError> {
    use base64::Engine as _;
    let outer: Vec<serde_json::Value> =
        serde_json::from_slice(raw).map_err(|_| WaaError("Waa/Create: not JSON".into()))?;
    let encoded = outer.get(1).and_then(|v| v.as_str()).unwrap_or_default();
    let fields: Vec<serde_json::Value> = if !encoded.is_empty() {
        let mut decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| WaaError("Waa/Create: bad base64 challenge".into()))?;
        for b in decoded.iter_mut() {
            *b = b.wrapping_add(97);
        }
        serde_json::from_slice(&decoded).map_err(|_| WaaError("Waa/Create: bad challenge json".into()))?
    } else {
        outer.first().and_then(|v| v.as_array()).cloned().unwrap_or_default()
    };
    if fields.len() < 8 {
        return err("Waa/Create: too few challenge fields");
    }
    let interpreter_path = jspb_first_str(&fields[2]);
    let interpreter_url = if interpreter_path.is_empty() {
        String::new()
    } else {
        format!("https:{}", interpreter_path.trim_start_matches("https:"))
    };
    let interpreter_hash = jspb_str(&fields[3]);
    let challenge = Challenge {
        program: jspb_str(&fields[4]),
        global_name: jspb_str(&fields[5]),
        interpreter_url,
    };
    if challenge.program.is_empty() || challenge.global_name.is_empty() || interpreter_hash.is_empty() {
        return err("Waa/Create: missing program/globalName/interpreter");
    }
    Ok(challenge)
}

/// Build the top-realm profile: the captured Firefox-152/Win64 fingerprint with
/// this session's cookies and the page URL's model.
pub fn build_top_profile(cookie: &str, model_short: &str) -> Result<String, WaaError> {
    let mut p: serde_json::Value =
        serde_json::from_str(PROFILE_TEMPLATE).map_err(|_| WaaError("profile template".into()))?;
    if let Some(doc) = p.pointer_mut("/values/document").and_then(|v| v.as_object_mut()) {
        doc.insert("cookie".into(), serde_json::Value::String(cookie.to_string()));
    }
    let href = format!("https://aistudio.google.com/prompts/new_chat?model={model_short}");
    if let Some(loc) = p.get_mut("location").and_then(|v| v.as_object_mut()) {
        loc.insert("href".into(), serde_json::Value::String(href));
        loc.insert("search".into(), serde_json::Value::String(format!("?model={model_short}")));
    }
    Ok(p.to_string())
}

/// Derive the binding prompt (proof digest source) and page prompt (typed text)
/// from a GenerateContent `contents` array, mirroring the reference signer.
pub fn extract_prompts(contents: &serde_json::Value, fallback: &str) -> (String, String) {
    let turns = match contents.as_array() {
        Some(t) if !t.is_empty() => t,
        _ => return (fallback.to_string(), fallback.to_string()),
    };
    let mut binding_parts: Vec<String> = Vec::new();
    let mut last_turn_text: Vec<String> = Vec::new();
    let last = turns.len() - 1;
    for (i, turn) in turns.iter().enumerate() {
        let parts = match turn.as_array().and_then(|a| a.first()).and_then(|v| v.as_array()) {
            Some(p) => p,
            None => continue,
        };
        let is_last = i == last;
        for part in parts {
            let pa = match part.as_array() {
                Some(a) => a,
                None => continue,
            };
            let thought = pa.first().and_then(|v| v.as_bool()).unwrap_or(false)
                || pa.get(10).and_then(|v| v.as_bool()).unwrap_or(false)
                || pa.get(12).and_then(|v| v.as_bool()).unwrap_or(false)
                || pa.get(12).and_then(|v| v.as_i64()) == Some(1);
            if let Some(text) = pa.get(1).and_then(|v| v.as_str()) {
                binding_parts.push(text.to_string());
                if is_last && !thought && !text.is_empty() {
                    last_turn_text.push(text.to_string());
                }
                continue;
            }
            if let Some(inline) = pa.get(2).and_then(|v| v.as_array())
                && let Some(b64) = inline.get(1).and_then(|v| v.as_str()) {
                    binding_parts.push(b64.to_string());
                    continue;
                }
            if let Some(file) = pa.get(5).and_then(|v| v.as_array())
                && let Some(id) = file.first().and_then(|v| v.as_str()) {
                    binding_parts.push(id.to_string());
                    continue;
                }
            binding_parts.push(String::new());
        }
    }
    let binding = if !fallback.is_empty() {
        fallback.to_string()
    } else {
        binding_parts.join(" ")
    };
    (binding, last_turn_text.join(" "))
}

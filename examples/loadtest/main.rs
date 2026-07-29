//! In-process load-test harness for the content-generation hot path.
//!
//! Drives `gproxy::pipeline::execute` directly against a mock upstream over the
//! full inbound×upstream×stream matrix (4 wires each way = 32 combinations).
//!
//! Modes:
//!   --smoke                          one request per combination, verify 2xx
//!   --matrix                         per-combination throughput table
//!   --ramp --inbound X --upstream Y  concurrency ramp for one combination

mod bench;
mod matrix;
mod metrics;
mod micro;
mod mock;
mod seed;

use std::sync::Arc;
use std::time::Duration;

use bench::{drive, run_load};
use matrix::{Combo, RequestTemplate, Wire};
use metrics::{RssSampler, kb_to_mb};
use mock::MockUpstream;

struct Args {
    smoke: bool,
    matrix: bool,
    ramp: bool,
    micro: bool,
    concurrency: usize,
    duration: u64,
    events: usize,
    inbound: Wire,
    upstream: Wire,
    stream: bool,
}

const USAGE: &str = "usage: loadtest --smoke | --matrix [--concurrency C] [--duration SECS] \
                     | --ramp --inbound W --upstream W [--stream] [--duration SECS] \
                     | --micro   [--events N]   (W = chat|responses|claude|gemini)";

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        smoke: false,
        matrix: false,
        ramp: false,
        micro: false,
        concurrency: 64,
        duration: 3,
        events: 100,
        inbound: Wire::Chat,
        upstream: Wire::Chat,
        stream: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--smoke" => args.smoke = true,
            "--matrix" => args.matrix = true,
            "--ramp" => args.ramp = true,
            "--micro" => args.micro = true,
            "--stream" => args.stream = true,
            "--concurrency" => {
                args.concurrency = value("--concurrency")?
                    .parse()
                    .map_err(|e| format!("{e}"))?
            }
            "--duration" => {
                args.duration = value("--duration")?.parse().map_err(|e| format!("{e}"))?
            }
            "--events" => args.events = value("--events")?.parse().map_err(|e| format!("{e}"))?,
            "--inbound" => {
                let v = value("--inbound")?;
                args.inbound = Wire::parse(&v).ok_or(format!("bad wire: {v}"))?;
            }
            "--upstream" => {
                let v = value("--upstream")?;
                args.upstream = Wire::parse(&v).ok_or(format!("bad wire: {v}"))?;
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    if !(args.smoke || args.matrix || args.ramp || args.micro) {
        return Err("pick a mode".into());
    }
    Ok(args)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            println!("error: {e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if args.micro {
        micro::run();
        return;
    }
    let mock = Arc::new(MockUpstream::new(args.events));
    let (state, _dir) = seed::build_state(mock).await;

    let failed = if args.smoke {
        smoke(&state).await
    } else if args.matrix {
        matrix_mode(&state, &args).await
    } else {
        ramp_mode(&state, &args).await
    };
    if failed {
        std::process::exit(1);
    }
}

/// One request per combination; every one must be 2xx (streams must relay >0
/// bytes). This is the correctness gate for the whole harness.
async fn smoke(state: &Arc<gproxy::app::AppState>) -> bool {
    println!("{:<28} {:>8}  result", "combo", "bytes");
    let mut failed = false;
    for combo in Combo::all() {
        let tpl = RequestTemplate::new(&combo);
        let ctx = tpl.ctx(bench::REQ_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        match drive(state, ctx).await {
            Ok(bytes) if bytes > 0 => {
                println!("{:<28} {:>8}  OK", combo.label(), bytes);
            }
            Ok(bytes) => {
                failed = true;
                println!("{:<28} {:>8}  FAIL (empty body)", combo.label(), bytes);
            }
            Err(e) => {
                failed = true;
                println!("{:<28} {:>8}  FAIL ({e})", combo.label(), "-");
            }
        }
    }
    println!(
        "\nsmoke: {}",
        if failed {
            "FAIL"
        } else {
            "all 32 combinations OK"
        }
    );
    failed
}

/// Per-combination timed windows at fixed concurrency; req/s compared against
/// the first combination (baseline).
async fn matrix_mode(state: &Arc<gproxy::app::AppState>, args: &Args) -> bool {
    println!(
        "matrix: concurrency={} duration={}s events={}",
        args.concurrency, args.duration, args.events
    );
    println!(
        "{:<28} {:>9} {:>10} {:>9} {:>11} {:>6}",
        "combo", "req/s", "MB/s", "vs-base", "peakΔ MB", "errs"
    );
    let sampler = RssSampler::start();
    let window = Duration::from_secs(args.duration);
    let mut baseline: Option<f64> = None;
    let mut total_errors = 0u64;
    for combo in Combo::all() {
        let tpl = Arc::new(RequestTemplate::new(&combo));
        let r = run_load(
            state,
            &tpl,
            args.concurrency,
            Duration::from_secs(1),
            window,
            &sampler,
        )
        .await;
        let base = *baseline.get_or_insert(r.req_s);
        total_errors += r.errors;
        println!(
            "{:<28} {:>9.0} {:>10.2} {:>8.2}x {:>11.1} {:>6}",
            combo.label(),
            r.req_s,
            r.mb_s,
            r.req_s / base,
            r.peak_delta_mb,
            r.errors
        );
    }
    sampler.stop().await;
    println!("final RSS: {:.1} MB", kb_to_mb(metrics::rss_kb()));
    if total_errors > 0 {
        println!("matrix: {total_errors} request(s) failed");
    }
    total_errors > 0
}

/// Concurrency ramp (64, 128, …) for one combination. Stops when peak RSS
/// exceeds 1 GiB or req/s drops >5% below the previous level.
async fn ramp_mode(state: &Arc<gproxy::app::AppState>, args: &Args) -> bool {
    let combo = Combo {
        inbound: args.inbound,
        upstream: args.upstream,
        stream: args.stream,
    };
    println!(
        "ramp: {} duration={}s events={}",
        combo.label(),
        args.duration,
        args.events
    );
    println!(
        "{:>11} {:>9} {:>10} {:>12} {:>6}",
        "concurrency", "req/s", "MB/s", "peak RSS MB", "errs"
    );
    let tpl = Arc::new(RequestTemplate::new(&combo));
    let sampler = RssSampler::start();
    let window = Duration::from_secs(args.duration);
    let mut concurrency = 64usize;
    let mut prev_req_s: Option<f64> = None;
    let mut total_errors = 0u64;
    loop {
        let r = run_load(
            state,
            &tpl,
            concurrency,
            Duration::from_secs(1),
            window,
            &sampler,
        )
        .await;
        total_errors += r.errors;
        println!(
            "{:>11} {:>9.0} {:>10.2} {:>12.1} {:>6}",
            concurrency,
            r.req_s,
            r.mb_s,
            kb_to_mb(r.peak_kb),
            r.errors
        );
        if r.peak_kb > 1024 * 1024 {
            println!("stop: peak RSS exceeded 1 GiB");
            break;
        }
        if let Some(prev) = prev_req_s
            && r.req_s < prev * 0.95
        {
            println!("stop: req/s dropped >5% vs previous level");
            break;
        }
        prev_req_s = Some(r.req_s);
        concurrency *= 2;
    }
    sampler.stop().await;
    total_errors > 0
}

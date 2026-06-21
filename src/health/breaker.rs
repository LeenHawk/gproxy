//! One passive circuit breaker (§3.2). All methods take `now` (unix secs) —
//! no clock inside, so tests drive time.
//!
//! Error-rate window is a two-bucket approximation: a current and a previous
//! `window_secs`-sized bucket; the rate is computed over both. Buckets roll
//! lazily on failure (success has no config in scope), which is within the
//! tolerance of the approximation.

use super::config::{BreakerConfig, COOLDOWN_CAP_SECS};

/// An outstanding half-open probe re-arms after this long. Covers probe slots
/// consumed during candidate filtering whose attempt never fires (an earlier
/// candidate served the request, empty credential pool, budget skip) — without
/// this the breaker would stay `HalfOpen { probing }` forever.
const PROBE_REARM_SECS: i64 = 30;

/// Admission verdict for the next request through this breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    Yes,
    /// Half-open: caller may send exactly one probe request.
    Probe,
    No {
        until: i64,
    },
}

/// What changed — the caller persists edges (§16.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    Opened {
        until: i64,
        consecutive_failures: u32,
    },
    Reopened {
        until: i64,
    },
    Closed,
}

#[derive(Debug)]
enum State {
    Closed {
        consecutive: u32,
        win_start: i64,
        win_ok: u32,
        win_fail: u32,
        prev_ok: u32,
        prev_fail: u32,
    },
    Open {
        until: i64,
        /// Consecutive opens since the last successful close; drives the
        /// cooldown multiplier `cooldown_secs * 2^(opens-1)`.
        opens: u32,
    },
    HalfOpen {
        /// While a probe is outstanding, the time at which the slot re-arms
        /// if its outcome was never recorded.
        probe_deadline: i64,
        opens: u32,
    },
}

fn closed(now: i64) -> State {
    State::Closed {
        consecutive: 0,
        win_start: now,
        win_ok: 0,
        win_fail: 0,
        prev_ok: 0,
        prev_fail: 0,
    }
}

fn cooldown_secs(cfg: &BreakerConfig, opens: u32) -> i64 {
    let exp = opens.saturating_sub(1).min(32);
    cfg.cooldown_secs
        .saturating_mul(1u64 << exp)
        .min(COOLDOWN_CAP_SECS) as i64
}

#[derive(Debug)]
pub struct Breaker {
    state: State,
}

impl Default for Breaker {
    fn default() -> Self {
        Self::new()
    }
}

impl Breaker {
    pub fn new() -> Self {
        Self { state: closed(0) }
    }

    /// Open + expired → flip to HalfOpen and return `Probe`. While a probe is
    /// outstanding further callers get `No { until: probe_deadline }`; if the
    /// probe outcome is never recorded (the slot was consumed during candidate
    /// filtering but the attempt never fired), the slot re-arms at the
    /// deadline and the next caller gets a fresh `Probe`.
    pub fn admit(&mut self, _cfg: &BreakerConfig, now: i64) -> Admit {
        match &mut self.state {
            State::Closed { .. } => Admit::Yes,
            State::Open { until, opens } => {
                if now >= *until {
                    self.state = State::HalfOpen {
                        probe_deadline: now + PROBE_REARM_SECS,
                        opens: *opens,
                    };
                    Admit::Probe
                } else {
                    Admit::No { until: *until }
                }
            }
            State::HalfOpen { probe_deadline, .. } => {
                if now >= *probe_deadline {
                    *probe_deadline = now + PROBE_REARM_SECS;
                    Admit::Probe
                } else {
                    Admit::No {
                        until: *probe_deadline,
                    }
                }
            }
        }
    }

    /// HalfOpen probe success → Closed (counters and cooldown multiplier
    /// reset); Closed → reset the consecutive-failure counter.
    pub fn on_success(&mut self, now: i64) -> Option<Transition> {
        match &mut self.state {
            State::Closed {
                consecutive,
                win_ok,
                ..
            } => {
                *consecutive = 0;
                *win_ok = win_ok.saturating_add(1);
                None
            }
            State::HalfOpen { .. } => {
                self.state = closed(now);
                Some(Transition::Closed)
            }
            // Late completion of a request issued before the open; ignore.
            State::Open { .. } => None,
        }
    }

    pub fn on_failure(&mut self, cfg: &BreakerConfig, now: i64) -> Option<Transition> {
        match &mut self.state {
            State::Closed {
                consecutive,
                win_start,
                win_ok,
                win_fail,
                prev_ok,
                prev_fail,
            } => {
                if let Some(er) = &cfg.error_rate {
                    let w = er.window_secs.max(1) as i64;
                    let elapsed = now - *win_start;
                    if elapsed >= 2 * w {
                        (*prev_ok, *prev_fail, *win_ok, *win_fail) = (0, 0, 0, 0);
                        *win_start = now;
                    } else if elapsed >= w {
                        (*prev_ok, *prev_fail) = (*win_ok, *win_fail);
                        (*win_ok, *win_fail) = (0, 0);
                        *win_start = now;
                    }
                }
                *consecutive = consecutive.saturating_add(1);
                *win_fail = win_fail.saturating_add(1);

                let consec_trip = *consecutive >= cfg.consecutive_failures;
                let rate_trip = cfg.error_rate.as_ref().is_some_and(|er| {
                    let fails = u64::from(*win_fail) + u64::from(*prev_fail);
                    let total = fails + u64::from(*win_ok) + u64::from(*prev_ok);
                    total >= u64::from(er.min_requests)
                        && fails as f64 / total as f64 >= er.threshold
                });
                if consec_trip || rate_trip {
                    let until = now + cooldown_secs(cfg, 1);
                    let consecutive_failures = *consecutive;
                    self.state = State::Open { until, opens: 1 };
                    Some(Transition::Opened {
                        until,
                        consecutive_failures,
                    })
                } else {
                    None
                }
            }
            State::HalfOpen { opens, .. } => {
                let opens = opens.saturating_add(1);
                let until = now + cooldown_secs(cfg, opens);
                self.state = State::Open { until, opens };
                Some(Transition::Reopened { until })
            }
            State::Open { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::ErrorRateCfg;
    use super::*;

    #[test]
    fn opens_after_consecutive_failures() {
        let cfg = BreakerConfig::default(); // 5 failures, 30s cooldown
        let mut b = Breaker::new();
        for _ in 0..4 {
            assert_eq!(b.on_failure(&cfg, 100), None);
        }
        assert_eq!(
            b.on_failure(&cfg, 100),
            Some(Transition::Opened {
                until: 130,
                consecutive_failures: 5
            })
        );
        assert_eq!(b.admit(&cfg, 100), Admit::No { until: 130 });
    }

    #[test]
    fn half_open_admits_one_probe_and_closes_on_success() {
        let cfg = BreakerConfig::default();
        let mut b = Breaker::new();
        for _ in 0..5 {
            b.on_failure(&cfg, 100);
        }
        assert_eq!(b.admit(&cfg, 130), Admit::Probe);
        assert_eq!(b.admit(&cfg, 130), Admit::No { until: 160 });
        assert_eq!(b.on_success(131), Some(Transition::Closed));
        assert_eq!(b.admit(&cfg, 131), Admit::Yes);
    }

    /// Regression: a probe slot consumed during candidate filtering whose
    /// attempt never fires must re-arm, not wedge the breaker in HalfOpen.
    #[test]
    fn unresolved_probe_rearms_after_deadline() {
        let cfg = BreakerConfig::default();
        let mut b = Breaker::new();
        for _ in 0..5 {
            b.on_failure(&cfg, 100);
        }
        assert_eq!(b.admit(&cfg, 130), Admit::Probe);
        // Probe outcome never recorded (another member served the request).
        assert_eq!(b.admit(&cfg, 159), Admit::No { until: 160 });
        assert_eq!(b.admit(&cfg, 160), Admit::Probe, "slot re-arms");
        assert_eq!(b.on_success(161), Some(Transition::Closed));
    }

    #[test]
    fn probe_failure_doubles_cooldown_and_caps() {
        let cfg = BreakerConfig {
            cooldown_secs: 100,
            ..BreakerConfig::default()
        };
        let mut b = Breaker::new();
        for _ in 0..5 {
            b.on_failure(&cfg, 0); // opens: until = 100
        }
        assert_eq!(b.admit(&cfg, 100), Admit::Probe);
        assert_eq!(
            b.on_failure(&cfg, 100),
            Some(Transition::Reopened { until: 300 }) // 2x100
        );
        assert_eq!(b.admit(&cfg, 300), Admit::Probe);
        assert_eq!(
            b.on_failure(&cfg, 300),
            Some(Transition::Reopened { until: 600 }) // 4x100 capped at 300
        );
    }

    #[test]
    fn error_rate_trips_only_past_min_requests() {
        let cfg = BreakerConfig {
            consecutive_failures: 100, // never trips on consecutive
            error_rate: Some(ErrorRateCfg {
                window_secs: 60,
                threshold: 0.5,
                min_requests: 5,
            }),
            cooldown_secs: 30,
        };
        let mut b = Breaker::new();
        // 4 samples at 50% — below min_requests, stays closed.
        for _ in 0..2 {
            b.on_success(10);
            assert_eq!(b.on_failure(&cfg, 10), None);
        }
        assert_eq!(b.admit(&cfg, 10), Admit::Yes);
        // 6th sample: 3 fails / 6 total = 0.5 >= threshold → opens.
        b.on_success(10);
        assert!(matches!(
            b.on_failure(&cfg, 10),
            Some(Transition::Opened { .. })
        ));
    }
}

use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::discovery;

pub const DEFAULT_IDLE_MINUTES: u64 = 10;
pub const DEFAULT_LEASE_SECONDS: u64 = 90;
const MAX_LEASES: usize = 64;

struct Lease {
    id: String,
    client: String,
    opened: Instant,
    expires: Instant,
}

struct State {
    last: Instant,
    leases: Vec<Lease>,
    stop: bool,
}

static STATE: LazyLock<Mutex<State>> =
    LazyLock::new(|| Mutex::new(State { last: Instant::now(), leases: Vec::new(), stop: false }));

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn prune(s: &mut State, now: Instant) {
    let mut latest = s.last;
    s.leases.retain(|lease| {
        if lease.expires > now {
            true
        } else {
            latest = latest.max(lease.expires);
            false
        }
    });
    s.last = latest;
}

fn seconds_env(name: &str) -> Option<u64> {
    env::var(name).ok().and_then(|v| v.trim().parse::<u64>().ok())
}

pub fn idle_limit() -> Option<Duration> {
    if let Some(seconds) = seconds_env("FORMSLOT_IDLE_SECONDS") {
        return (seconds > 0).then(|| Duration::from_secs(seconds));
    }
    let minutes = seconds_env("FORMSLOT_IDLE_MINUTES").unwrap_or(DEFAULT_IDLE_MINUTES);
    (minutes > 0).then(|| Duration::from_secs(minutes * 60))
}

pub fn lease_ttl() -> Duration {
    Duration::from_secs(seconds_env("FORMSLOT_LEASE_SECONDS").filter(|s| *s > 0).unwrap_or(DEFAULT_LEASE_SECONDS))
}

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

pub struct Busy;

pub fn busy() -> Busy {
    IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
    touch();
    Busy
}

impl Drop for Busy {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        touch();
    }
}

pub fn touch() {
    let mut s = state();
    let now = Instant::now();
    prune(&mut s, now);
    s.last = now;
}

pub fn request_stop() {
    state().stop = true;
}

pub fn stop_requested() -> bool {
    state().stop
}

pub fn client_ok(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= 64 && !trimmed.chars().any(char::is_control)
}

fn view(lease: &Lease, now: Instant, ttl: Duration) -> Value {
    json!({
        "lease": lease.id,
        "client": lease.client,
        "ttl_seconds": ttl.as_secs(),
        "heartbeat_seconds": (ttl.as_secs() / 3).max(1),
        "expires_in_seconds": lease.expires.saturating_duration_since(now).as_secs(),
    })
}

pub fn open(client: &str) -> Result<Value, String> {
    let mut s = state();
    let now = Instant::now();
    prune(&mut s, now);
    s.last = now;
    if s.leases.len() >= MAX_LEASES {
        return Err(format!("{MAX_LEASES} leases are open already"));
    }
    let ttl = lease_ttl();
    let lease = Lease {
        id: discovery::new_token()[..16].to_string(),
        client: client.trim().to_string(),
        opened: now,
        expires: now + ttl,
    };
    let out = view(&lease, now, ttl);
    s.leases.push(lease);
    Ok(out)
}

pub fn renew(id: &str) -> Option<Value> {
    let mut s = state();
    let now = Instant::now();
    prune(&mut s, now);
    s.last = now;
    let ttl = lease_ttl();
    let lease = s.leases.iter_mut().find(|l| l.id == id)?;
    lease.expires = now + ttl;
    Some(view(lease, now, ttl))
}

pub fn release(id: &str) -> bool {
    let mut s = state();
    let now = Instant::now();
    prune(&mut s, now);
    s.last = now;
    let before = s.leases.len();
    s.leases.retain(|l| l.id != id);
    s.leases.len() != before
}

pub fn snapshot() -> Value {
    let mut s = state();
    let now = Instant::now();
    prune(&mut s, now);
    let leases: Vec<Value> = s
        .leases
        .iter()
        .map(|l| {
            json!({
                "client": l.client,
                "age_seconds": now.saturating_duration_since(l.opened).as_secs(),
                "expires_in_seconds": l.expires.saturating_duration_since(now).as_secs(),
            })
        })
        .collect();
    json!({
        "leases": leases,
        "idle_seconds": now.saturating_duration_since(s.last).as_secs(),
        "idle_limit_seconds": idle_limit().map(|d| d.as_secs()),
        "lease_ttl_seconds": lease_ttl().as_secs(),
    })
}

const IDLE_GRACE: Duration = Duration::from_millis(500);
const DRAIN_WAIT: Duration = Duration::from_secs(5);

fn decide() -> Option<&'static str> {
    let mut s = state();
    if s.stop {
        return Some("requested");
    }
    let now = Instant::now();
    prune(&mut s, now);
    if crate::http::open_connections() > 0 {
        s.last = now;
    }
    let limit = idle_limit()?;
    if IN_FLIGHT.load(Ordering::SeqCst) == 0 && s.leases.is_empty() && now.saturating_duration_since(s.last) >= limit {
        return Some("idle");
    }
    None
}

pub fn exit_reason() -> Option<&'static str> {
    match decide() {
        Some("idle") => {
            std::thread::sleep(IDLE_GRACE);
            decide()
        }
        other => other,
    }
}

pub fn drain() -> bool {
    let began = Instant::now();
    while IN_FLIGHT.load(Ordering::SeqCst) > 0 || crate::http::open_connections() > 0 {
        if began.elapsed() >= DRAIN_WAIT {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

//! Prototype: the server's move check for a player carried by a moving collider.
//!
//! A simulation with no game: a collider deck moves on the server at 20 Hz, a client sees it
//! late through the entity interpolation (3 steps), carries its own player on that copy, and
//! reports the position back late. The server judges each report in the collider's local frame.
//! It keeps the last accepted local position and the age (ticks before the newest) of the
//! transform that report matched. A report is accepted if some stored transform in the age
//! window puts it on a box top (within `SUPPORT_TOL`), not inside a box, over the deck, and
//! within the local move limit (`WALK_MAX` + `tol`) of the last accepted local position; among
//! the candidates the one whose age is nearest the expected age is taken. The expected age is
//! what the round-trip time predicts (`2 * one-way ticks + LERP_LAG`), and the first report
//! starts from it. Anchoring at the expected age matters: the lag along the collider's motion
//! cannot be told from the player's own walk, so a rule that follows the previous age lets its
//! age wander until it leaves the window.
//!
//! `cargo run --release -p sunrise-core --example collider_check_sim -- <out-dir>`
//! writes `table.csv` (every configuration), `tables.md` and `cost.md`.

use std::fmt::Write as _;
use std::time::Instant;
use sunrise_core::geometry::{Aabb, Axis, Vec3};

/// Server ticks per second.
const TICK_MS: u32 = 50;
/// Steps of `LinearInterpolationHandler` (`DEFAULT_INTERPOLATION_STEPS`).
const LERP_STEPS: u32 = 3;
/// The fastest honest walk in the collider's frame, blocks per tick (sprinting is 0.28).
const WALK_MAX: f64 = 0.28;
/// Ticks before the counted run, so the pipelines are full.
const PREROLL: i64 = 300;
/// Counted ticks per configuration.
const RUN: i64 = 20000;
/// Vertical tolerance of "stands on a box top".
const SUPPORT_TOL: f64 = 0.05;
/// Ticks the client's 3-step interpolation adds to the one-way delay (steady state).
const LERP_LAG: i64 = 2;

#[derive(Clone, Copy, Debug)]
struct Tf {
    p: Vec3,
    yaw: f64,
}

impl Tf {
    fn world(self, u: Vec3) -> Vec3 {
        let (s, c) = self.yaw.sin_cos();
        Vec3::new(
            self.p.x + u.x * c + u.z * s,
            self.p.y + u.y,
            self.p.z - u.x * s + u.z * c,
        )
    }
    fn local(self, w: Vec3) -> Vec3 {
        let (s, c) = self.yaw.sin_cos();
        let d = Vec3::new(w.x - self.p.x, w.y - self.p.y, w.z - self.p.z);
        Vec3::new(d.x * c - d.z * s, d.y, d.x * s + d.z * c)
    }
    fn lerp(self, o: Tf, t: f64) -> Tf {
        Tf {
            p: self.p.lerp(o.p, t),
            yaw: self.yaw + (o.yaw - self.yaw) * t,
        }
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn jitter(&mut self) -> i64 {
        (self.next() % 3) as i64 - 1
    }
}

#[derive(Clone, Copy)]
struct Path {
    speed: f64,
    /// Yaw change per tick, radians.
    turn: f64,
}

/// The collider's transform after server tick `t` (defined for negative ticks too).
fn server_state(path: Path, t: i64) -> Tf {
    // Straight: along +z. Curve: the heading turns by `turn` per tick and the origin
    // follows it, so the deck sweeps an arc.
    if path.turn == 0.0 {
        return Tf {
            p: Vec3::new(0.0, 64.0, path.speed * t as f64),
            yaw: 0.0,
        };
    }
    // Closed form of the arc: radius r = speed / turn.
    let r = path.speed / path.turn;
    let a = path.turn * t as f64;
    Tf {
        p: Vec3::new(r * (1.0 - a.cos()), 64.0, r * a.sin()),
        yaw: a,
    }
}

/// The collider's deck, in its local frame: one 3 x 1 x 20 box whose top is at y = 0.
fn deck() -> Aabb {
    Aabb::new(Vec3::new(-1.5, -1.0, -10.0), Vec3::new(1.5, 0.0, 10.0))
}

/// The server's stored history of one collider: the last `n` transforms, newest last.
struct History {
    ring: Vec<Tf>,
    next: usize,
    filled: usize,
}
impl History {
    fn new(n: usize) -> Self {
        let z = Tf {
            p: Vec3::ZERO,
            yaw: 0.0,
        };
        Self {
            ring: vec![z; n],
            next: 0,
            filled: 0,
        }
    }
    fn push(&mut self, tf: Tf) {
        self.ring[self.next] = tf;
        self.next = (self.next + 1) % self.ring.len();
        self.filled = (self.filled + 1).min(self.ring.len());
    }
    /// The transform `age` ticks before the newest, with fractional ages interpolated.
    fn at(&self, age: f64) -> Tf {
        let n = self.ring.len();
        let i = age.floor() as usize;
        let f = age - i as f64;
        let get = |k: usize| self.ring[(self.next + n - 1 - k.min(n - 1)) % n];
        if f == 0.0 {
            get(i)
        } else {
            get(i + 1).lerp(get(i), 1.0 - f)
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Rule {
    /// Only the stored ticks are candidates.
    Discrete,
    /// Candidates run continuously between stored ticks.
    Continuous,
}

struct Verdict {
    ok: bool,
    /// The chosen candidate age (ticks before the newest).
    age: f64,
    u: Vec3,
}

/// The rule. Candidate ages lie in `[lo, hi]`; `limit` is the local move limit for this packet.
fn judge(
    h: &History,
    (lo, hi): (f64, f64),
    rule: Rule,
    w: Vec3,
    last: Vec3,
    anchor: f64,
    limit: f64,
) -> Verdict {
    let fits = |age: f64| {
        let u = h.at(age).local(w);
        if u.distance(last) > limit {
            return None;
        }
        // On the top within tolerance, over the deck, not inside it deeper than the tolerance.
        let footprint = Aabb::of_size(Vec3::new(u.x, u.y + 0.9, u.z), Vec3::new(0.85, 1.8, 0.85));
        let over_deck = u.x.abs() <= 1.5 + 0.3 && u.z.abs() <= 10.0 + 0.3;
        let on_top = u.y.abs() <= SUPPORT_TOL;
        let inside = footprint.intersects(deck().with_max(Axis::Y, -SUPPORT_TOL));
        (over_deck && on_top && !inside).then_some(u)
    };
    let found = match rule {
        Rule::Discrete => {
            let (a, b) = (lo.ceil() as i64, hi.floor() as i64);
            (a..=b)
                .map(|k| k as f64)
                .filter_map(|age| fits(age).map(|u| (age, u)))
                .min_by(|x, y| (x.0 - anchor).abs().total_cmp(&(y.0 - anchor).abs()))
        }
        Rule::Continuous => {
            // Outward from the anchor, in sixteenths of a tick.
            let start = anchor.clamp(lo, hi);
            let mut hit = None;
            let steps = ((hi - lo) * 16.0).ceil() as usize + 1;
            for j in 0..=steps {
                let d = j as f64 / 16.0;
                for age in [start - d, start + d] {
                    if (lo..=hi).contains(&age)
                        && let Some(u) = fits(age)
                    {
                        hit = Some((age, u));
                        break;
                    }
                }
                if hit.is_some() {
                    break;
                }
            }
            hit
        }
    };
    match found {
        Some((age, u)) => Verdict { ok: true, age, u },
        None => Verdict {
            ok: false,
            age: anchor,
            u: last,
        },
    }
}

/// The client's interpolated copy (`LinearInterpolationHandler`).
struct ClientCopy {
    now: Tf,
    target: Tf,
    steps: u32,
}
impl ClientCopy {
    fn receive(&mut self, tf: Tf) {
        if self.steps == 0 || self.target.p != tf.p || self.target.yaw != tf.yaw {
            self.target = tf;
            self.steps = LERP_STEPS;
        }
    }
    fn tick(&mut self) {
        if self.steps > 0 {
            let t = 1.0 / f64::from(self.steps);
            self.now = self.now.lerp(self.target, t);
            self.steps -= 1;
        }
    }
}

#[derive(Clone, Copy)]
enum Window {
    /// Every stored transform: ages 0 to n - 1.
    Full(usize),
    /// Ages within `d` ticks of the expected age (stores `expected + d + 1` transforms).
    Centered(usize),
}

impl Window {
    fn stored(self, expected: i64) -> usize {
        match self {
            Window::Full(n) => n,
            Window::Centered(d) => expected as usize + d + 1,
        }
    }
    fn ages(self, expected: i64) -> (f64, f64) {
        match self {
            Window::Full(n) => (0.0, (n - 1) as f64),
            Window::Centered(d) => (
                (expected - d as i64).max(0) as f64,
                (expected + d as i64) as f64,
            ),
        }
    }
}

#[derive(Clone, Copy)]
struct Config {
    path: Path,
    latency_ms: u32,
    jitter: bool,
    window: Window,
    rule: Rule,
    tol: f64,
    /// Error of the server's estimate of the expected age, in ticks.
    bias: i64,
}

#[derive(Default, Clone)]
struct Outcome {
    reports: u64,
    accepted: u64,
    /// Smallest and largest age matched by accepted reports.
    age_min: f64,
    age_max: f64,
    /// World distance between the newest and the oldest candidate frame for the player's local
    /// position (the one-shot slack of the window), at its largest and on average.
    slack_max: f64,
    slack_sum: f64,
}

fn run(cfg: Config) -> Outcome {
    let ld = i64::from(cfg.latency_ms / TICK_MS);
    let expected = (2 * ld + LERP_LAG + cfg.bias).max(0);
    let n = cfg.window.stored(expected);
    let (lo, hi) = cfg.window.ages(expected);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (n as u64) << 8 ^ u64::from(cfg.latency_ms));
    let jit = |r: &mut Rng| if cfg.jitter { r.jitter() } else { 0 };
    let mut out = Outcome {
        age_min: f64::MAX,
        ..Default::default()
    };

    let mut hist = History::new(n);
    let mut down: std::collections::VecDeque<(i64, Tf)> = Default::default();
    let mut up: std::collections::VecDeque<(i64, Vec3)> = Default::default();
    let (mut last_down, mut last_up) = (i64::MIN, i64::MIN);

    let s0 = server_state(cfg.path, -PREROLL);
    let mut copy = ClientCopy {
        now: s0,
        target: s0,
        steps: 0,
    };
    let mut u = Vec3::ZERO;
    // The server's last accepted local position and matched age; None until the first report.
    let mut server: Option<(Vec3, f64)> = None;
    let (mut heading, mut speed) = (0.0_f64, 0.1_f64);
    let limit = WALK_MAX + cfg.tol;

    for t in -PREROLL..RUN {
        let s = server_state(cfg.path, t);
        hist.push(s);
        let arrive = (t + ld + jit(&mut rng)).max(last_down).max(t);
        last_down = arrive;
        down.push_back((arrive, s));
        while up.front().is_some_and(|&(a, _)| a <= t) {
            let (_, w) = up.pop_front().unwrap();
            if hist.filled < n {
                continue;
            }
            let Some((su, _)) = server else {
                // First report: the age the round-trip time predicts.
                let age = expected.min(hi as i64) as f64;
                server = Some((hist.at(age).local(w), age));
                continue;
            };
            let v = judge(&hist, (lo, hi), cfg.rule, w, su, expected as f64, limit);
            if t >= 0 {
                out.reports += 1;
                let a = hist.at(lo).world(su);
                let b = hist.at(hi).world(su);
                let sl = a.distance(b);
                out.slack_max = out.slack_max.max(sl);
                out.slack_sum += sl;
            }
            if v.ok {
                server = Some((v.u, v.age));
                if t >= 0 {
                    out.accepted += 1;
                    out.age_min = out.age_min.min(v.age);
                    out.age_max = out.age_max.max(v.age);
                }
            } else {
                // Teleported back: the client's player returns to the server's local position.
                u = su;
            }
        }
        while down.front().is_some_and(|&(a, _)| a <= t) {
            copy.receive(down.pop_front().unwrap().1);
        }
        copy.tick();
        if t % 20 == 0 {
            speed = rng.unit() * WALK_MAX;
            heading += (rng.unit() - 0.5) * 3.0;
        }
        u = Vec3::new(
            u.x + speed * heading.sin(),
            0.0,
            u.z + speed * heading.cos(),
        );
        if u.x.abs() > 1.2 {
            u.x = u.x.clamp(-1.2, 1.2);
            heading = -heading;
        }
        if u.z.abs() > 9.5 {
            u.z = u.z.clamp(-9.5, 9.5);
            heading = std::f64::consts::PI - heading;
        }
        let w = copy.now.world(u);
        let arrive = (t + ld + jit(&mut rng)).max(last_up).max(t);
        last_up = arrive;
        up.push_back((arrive, w));
    }
    out
}

fn pct(o: &Outcome) -> f64 {
    100.0 * o.accepted as f64 / o.reports.max(1) as f64
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let deg = std::f64::consts::PI / 180.0;
    let straight = [0.5, 1.0, 1.4, 2.0];
    let curved = [0.5, 0.7, 1.0, 2.0];
    let mut paths = Vec::new();
    for v in straight {
        paths.push((
            "straight",
            Path {
                speed: v,
                turn: 0.0,
            },
        ));
    }
    for v in curved {
        paths.push((
            "curve 4deg",
            Path {
                speed: v,
                turn: 4.0 * deg,
            },
        ));
    }
    let lats = [50u32, 100, 200];
    let tols = [0.0, 0.05, 0.1, 0.25, 0.5, 1.0];
    // (label, window for a given latency) – Full(n) and Centered(d).
    let fulls = [6usize, 8, 10, 12, 16, 24];
    let centers = [2usize, 3, 4, 5, 6];

    let mut csv = String::from(
        "path,speed,latency_ms,rule,window,size,tol,reports,accepted_pct,age_min,age_max,slack_max,slack_mean\n",
    );
    let mut md = String::new();
    for rule in [Rule::Continuous, Rule::Discrete] {
        let rn = if rule == Rule::Continuous {
            "continuous"
        } else {
            "discrete"
        };
        for (pn, path) in &paths {
            for &lat in &lats {
                let exp = 2 * i64::from(lat / TICK_MS) + LERP_LAG;
                let _ = writeln!(
                    md,
                    "\n#### {rn}, {pn} {} b/tick, {lat} ms each way (expected age {exp}), jitter +-1 tick\n",
                    path.speed
                );
                let _ = write!(md, "| window (stored) \\ tol |");
                for t in tols {
                    let _ = write!(md, " {t} |");
                }
                let _ = writeln!(md, " age seen | world slack max (blocks) |");
                let _ = writeln!(md, "|---|{}---|---|", "---|".repeat(tols.len()));
                let mut wins: Vec<(String, Window)> = Vec::new();
                for &n in &fulls {
                    wins.push((format!("full N={n}"), Window::Full(n)));
                }
                for &d in &centers {
                    wins.push((format!("centered +-{d}"), Window::Centered(d)));
                }
                for (label, win) in wins {
                    let _ = write!(md, "| {label} ({}) |", win.stored(exp));
                    let (mut amin, mut amax, mut slack) = (0.0, 0.0, 0.0);
                    for &tol in &tols {
                        let o = run(Config {
                            path: *path,
                            latency_ms: lat,
                            jitter: true,
                            window: win,
                            rule,
                            tol,
                            bias: 0,
                        });
                        let _ = write!(md, " {:.2}% |", pct(&o));
                        let _ = writeln!(
                            csv,
                            "{pn},{},{lat},{rn},{label},{},{tol},{},{:.4},{:.3},{:.3},{:.3},{:.3}",
                            path.speed,
                            win.stored(exp),
                            o.reports,
                            pct(&o),
                            o.age_min,
                            o.age_max,
                            o.slack_max,
                            o.slack_sum / o.reports.max(1) as f64
                        );
                        (amin, amax, slack) = (o.age_min, o.age_max, o.slack_max);
                    }
                    let _ = writeln!(md, " {amin:.2}..{amax:.2} | {slack:.2} |");
                }
            }
        }
    }
    std::fs::write(format!("{dir}/table.csv"), csv).unwrap();
    std::fs::write(format!("{dir}/tables.md"), &md).unwrap();

    // Sensitivity to the server's estimate of the expected age (the round-trip time).
    let mut sens = String::from(
        "\n| path | one-way ms | window | tol | error -3 | -2 | -1 | 0 | +1 | +2 | +3 |\n|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for (pn, path) in [
        (
            "straight 2.0",
            Path {
                speed: 2.0,
                turn: 0.0,
            },
        ),
        (
            "curve 1.0",
            Path {
                speed: 1.0,
                turn: 4.0 * deg,
            },
        ),
    ] {
        for lat in [50u32, 100, 200] {
            for d in [3usize, 4, 5] {
                let _ = write!(sens, "| {pn} | {lat} | +-{d} | 0.25 |");
                for bias in -3..=3 {
                    let o = run(Config {
                        path,
                        latency_ms: lat,
                        jitter: true,
                        window: Window::Centered(d),
                        rule: Rule::Continuous,
                        tol: 0.25,
                        bias,
                    });
                    let _ = write!(sens, " {:.2}% |", pct(&o));
                }
                let _ = writeln!(sens);
            }
        }
    }
    std::fs::write(format!("{dir}/sensitivity.md"), &sens).unwrap();

    // Cost of one check against a stored history, per window size.
    let mut cost = String::from(
        "\n| stored transforms | rule | bytes per collider | ns per report |\n|---|---|---|---|\n",
    );
    for &n in &[6usize, 8, 10, 12, 16, 24] {
        for rule in [Rule::Discrete, Rule::Continuous] {
            let path = Path {
                speed: 2.0,
                turn: 4.0 * deg,
            };
            let mut h = History::new(n);
            for t in 0..n as i64 {
                h.push(server_state(path, t));
            }
            let age = ((n - 1) as f64) * 0.75;
            let u = Vec3::new(0.3, 0.0, 2.0);
            let w = h.at(age).world(u);
            let reps = 20_000;
            let start = Instant::now();
            let mut acc = 0.0;
            for _ in 0..reps {
                // The previous age is the farthest edge, the worst case for the search.
                let v = judge(
                    &h,
                    (0.0, (n - 1) as f64),
                    rule,
                    std::hint::black_box(w),
                    u,
                    0.0,
                    WALK_MAX,
                );
                acc += v.age;
            }
            std::hint::black_box(acc);
            let ns_per = start.elapsed().as_nanos() as f64 / f64::from(reps);
            let _ = writeln!(
                cost,
                "| {n} | {} | {} | {ns_per:.0} |",
                if rule == Rule::Discrete {
                    "discrete"
                } else {
                    "continuous"
                },
                n * std::mem::size_of::<Tf>()
            );
        }
    }
    std::fs::write(format!("{dir}/cost.md"), &cost).unwrap();
    println!("{cost}");
}

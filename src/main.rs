use clihelp::{HelpPage, Row, Section};
use std::hint::black_box;
use std::fmt::Write as FmtWrite;
use std::io::{self, IsTerminal, Write as IoWrite};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn cycles() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
fn cycles() -> u64 {
    let val: u64;
    unsafe { std::arch::asm!("mrs {}, cntvct_el0", out(reg) val) };
    val
}

fn cycles_per_ns() -> f64 {
    let dur = std::time::Duration::from_micros(50);
    let start_t = std::time::Instant::now();
    let start_c = cycles();
    while start_t.elapsed() < dur {
        std::hint::spin_loop();
    }
    let elapsed_c = (cycles() - start_c) as f64;
    let elapsed_ns = start_t.elapsed().as_nanos() as f64;
    elapsed_c / elapsed_ns
}

#[inline(always)]
fn count_range(start: u64, end: u64) -> u64 {
    if start >= end {
        0
    } else {
        black_box(end - start)
    }
}

#[repr(align(64))]
struct WorkerSlot {
    request: AtomicU64,
    start: AtomicU64,
    end: AtomicU64,
    done: AtomicU64,
    result: AtomicU64,
}

struct Worker {
    slot: Arc<WorkerSlot>,
}

struct WorkerPool {
    workers: Vec<Worker>,
    stop: Arc<AtomicBool>,
    handles: Vec<thread::JoinHandle<()>>,
    yield_wait: bool,
}

impl WorkerPool {
    fn new(n_threads: usize) -> Self {
        let yield_wait = n_threads > thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let mut workers = Vec::with_capacity(n_threads - 1);
        let mut handles = Vec::with_capacity(n_threads - 1);
        let stop = Arc::new(AtomicBool::new(false));
        for _ in 1..n_threads {
            let slot = Arc::new(WorkerSlot {
                request: AtomicU64::new(0),
                start: AtomicU64::new(0),
                end: AtomicU64::new(0),
                done: AtomicU64::new(0),
                result: AtomicU64::new(0),
            });
            let worker_slot = Arc::clone(&slot);
            let worker_stop = Arc::clone(&stop);
            let handle = thread::spawn(move || {
                let mut seen = 0u64;
                let mut spins = 0u32;
                loop {
                    let request = worker_slot.request.load(Ordering::Acquire);
                    if request == seen {
                        if worker_stop.load(Ordering::Relaxed) {
                            break;
                        }
                        if !yield_wait {
                            std::hint::spin_loop();
                            continue;
                        }
                        if spins < 32 {
                            std::hint::spin_loop();
                            spins += 1;
                        } else {
                            thread::yield_now();
                            spins = 0;
                        }
                        continue;
                    }
                    spins = 0;
                    let start = worker_slot.start.load(Ordering::Relaxed);
                    let end = worker_slot.end.load(Ordering::Relaxed);
                    let counted = count_range(start, end);
                    worker_slot.result.store(counted, Ordering::Relaxed);
                    worker_slot.done.store(request, Ordering::Release);
                    seen = request;
                }
            });
            workers.push(Worker { slot });
            handles.push(handle);
        }
        WorkerPool { workers, stop, handles, yield_wait }
    }

    fn count_to(&self, target: u64) -> u64 {
        if target == 0 {
            return 0;
        }
        let n = (self.workers.len() + 1).min(target as usize).max(1);
        let chunk = target.div_ceil(n as u64);
        let mut active = 0usize;
        for t in 1..n {
            let start = t as u64 * chunk;
            let end = (start + chunk).min(target);
            if start >= end {
                break;
            }
            let slot = &self.workers[t - 1].slot;
            let request = slot.request.load(Ordering::Relaxed) + 1;
            slot.start.store(start, Ordering::Relaxed);
            slot.end.store(end, Ordering::Relaxed);
            slot.request.store(request, Ordering::Release);
            active += 1;
        }
        let mut total = count_range(0, chunk.min(target));
        for worker in self.workers.iter().take(active) {
            let slot = &worker.slot;
            let request = slot.request.load(Ordering::Relaxed);
            let mut spins = 0u32;
            while slot.done.load(Ordering::Acquire) != request {
                if !self.yield_wait || spins < 32 {
                    std::hint::spin_loop();
                    spins = spins.wrapping_add(1);
                } else {
                    thread::yield_now();
                    spins = 0;
                }
            }
            total += slot.result.load(Ordering::Relaxed);
        }
        total
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.handles.clear();
    }
}

enum Counter {
    Direct,
    Spinning(WorkerPool),
}

impl Counter {
    fn new(threads: usize) -> Self {
        if threads <= 1 {
            Counter::Direct
        } else {
            Counter::Spinning(WorkerPool::new(threads))
        }
    }

    fn count_to(&self, target: u64) -> u64 {
        match self {
            Counter::Direct => count_range(0, target),
            Counter::Spinning(pool) => pool.count_to(target),
        }
    }
}

#[inline(never)]
fn time_one_trial(target: u64, counter: &Counter) -> (u64, u64) {
    let c0 = cycles();
    let result = counter.count_to(target);
    let c1 = cycles();
    (result, c1 - c0)
}

fn row(short: &'static str, long: &'static str, desc: &'static str) -> Row {
    Row::new(short, long, desc)
}
fn row_val(
    short: &'static str,
    long: &'static str,
    placeholder: &'static str,
    desc: &'static str,
) -> Row {
    Row::with_value(short, long, placeholder, desc)
}

fn counting_rows() -> Vec<Row> {
    vec![
        row_val("-c", "--count-to", "<N>", "count to N instead of 1 (must be an integer)"),
    ]
}

fn parallelism_rows() -> Vec<Row> {
    vec![
        row_val("-t", "--threads", "<N>", "threads for parallel counting (default: all cores)"),
    ]
}

fn benchmarking_rows() -> Vec<Row> {
    vec![
        row_val("-r", "--repeat", "<N>", "run N trials, report min/mean/median/max (default: 1)"),
    ]
}

fn output_rows() -> Vec<Row> {
    vec![
        row("-q", "--quiet", "print only the raw number(s), script-friendly"),
        row("-j", "--json", "machine-readable JSON output"),
        row_val("-u", "--unit", "<UNIT>", "ns | µs | ms | cycles  (default: ns)"),
        row_val("-L", "--color", "<WHEN>", "auto | always | never  (default: auto)"),
    ]
}

fn misc_rows() -> Vec<Row> {
    vec![
        row("-h", "--help", "show this help"),
        row("-v", "--version", "show version"),
    ]
}

fn sections() -> Vec<Section> {
    vec![
        Section {
            title: "COUNTING",
            note: Some("By default, fastcount counts to 1. Go on, live a little."),
            rows: counting_rows(),
        },
        Section {
            title: "PARALLELISM",
            note: Some("Range is chunked across threads; each chunk reduced in constant time."),
            rows: parallelism_rows(),
        },
        Section {
            title: "BENCHMARKING",
            note: Some("Repeated trials share one warm-up phase, then are timed independently."),
            rows: benchmarking_rows(),
        },
        Section { title: "OUTPUT", note: None, rows: output_rows() },
        Section { title: "MISC", note: None, rows: misc_rows() },
    ]
}

fn print_help() {
    print_help_body(io::stdout().is_terminal());
}

pub fn print_help_body(on: bool) {
    let mut page = HelpPage::new(format!(
        "{} {} - {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_DESCRIPTION")
    ))
    .usage("fastcount [options]")
    .blurb(
        "Counts from 0 up to a target number as fast as physically possible,\n\
         in parallel, with constant-time reduction, then stops, having\n\
         accomplished nothing of consequence considerably faster than it needed to.",
    )
    .footer("EXIT CODES: 0 = counted successfully  2 = invalid option value");

    for section in sections() {
        page = page.section(section);
    }

    print!("{}", page.render(on));
}

#[derive(Clone, Copy, PartialEq)]
enum Unit {
    Ns,
    Us,
    Ms,
    Cycles,
}

impl Unit {
    fn label(&self) -> &'static str {
        match self {
            Unit::Ns => "ns",
            Unit::Us => "us",
            Unit::Ms => "ms",
            Unit::Cycles => "cycles",
        }
    }

    fn convert(&self, cyc: u64, ns: f64) -> f64 {
        match self {
            Unit::Ns => ns,
            Unit::Us => ns / 1_000.0,
            Unit::Ms => ns / 1_000_000.0,
            Unit::Cycles => cyc as f64,
        }
    }
}

fn parse_unit(v: &str) -> Result<Unit, String> {
    match v {
        "ns" => Ok(Unit::Ns),
        "us" => Ok(Unit::Us),
        "ms" => Ok(Unit::Ms),
        "cycles" => Ok(Unit::Cycles),
        _ => Err(format!("unknown unit: '{v}' (expected ns, us, ms, or cycles)")),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

fn parse_color_when(v: &str) -> Result<ColorWhen, String> {
    match v {
        "auto" => Ok(ColorWhen::Auto),
        "always" => Ok(ColorWhen::Always),
        "never" => Ok(ColorWhen::Never),
        _ => Err(format!("unknown color mode: '{v}' (expected auto, always, or never)")),
    }
}

fn use_color(when: ColorWhen) -> bool {
    match when {
        ColorWhen::Always => true,
        ColorWhen::Never => false,
        ColorWhen::Auto => io::stdout().is_terminal(),
    }
}

struct Options {
    count_to: u64,
    threads: usize,
    repeat: u64,
    quiet: bool,
    json: bool,
    unit: Unit,
    color: ColorWhen,
}

impl Default for Options {
    fn default() -> Self {
        let default_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        Options {
            count_to: 1,
            threads: default_threads,
            repeat: 1,
            quiet: false,
            json: false,
            unit: Unit::Ns,
            color: ColorWhen::Auto,
        }
    }
}

fn parse_positive_int(flag: &str, v: &str) -> Result<u64, String> {
    let n: i64 = v
        .trim()
        .parse()
        .map_err(|_| format!("{flag} expects an integer, got '{v}'"))?;
    if n < 1 {
        return Err(format!("{flag} must be a positive integer (>= 1)"));
    }
    Ok(n as u64)
}

fn take_inline_value(
    arg: &str,
    bytes: &[u8],
    j: &mut usize,
    raw: &[String],
    i: &mut usize,
) -> String {
    let rest = &arg[2 + *j..];
    if !rest.is_empty() {
        *j = bytes.len();
        rest.to_string()
    } else {
        *i += 1;
        raw.get(*i).cloned().unwrap_or_default()
    }
}

fn parse_args_from(raw: &[String]) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut i = 0usize;

    while i < raw.len() {
        let arg = &raw[i];

        if arg.starts_with("--") {
            let key_val = &arg[2..];
            let (key, val_opt) = if let Some(eq) = key_val.find('=') {
                (&key_val[..eq], Some(&key_val[eq + 1..]))
            } else {
                (key_val, None)
            };

            let mut next_val = || -> String {
                if let Some(v) = val_opt {
                    v.to_string()
                } else {
                    i += 1;
                    raw.get(i).cloned().unwrap_or_default()
                }
            };

            match key {
                "help" => {
                    print_help();
                    std::process::exit(0);
                }
                "version" => {
                    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "count-to" => {
                    let v = next_val();
                    opts.count_to = parse_positive_int("--count-to", &v)?;
                }
                "threads" => {
                    let v = next_val();
                    opts.threads = parse_positive_int("--threads", &v)? as usize;
                }
                "repeat" => {
                    let v = next_val();
                    opts.repeat = parse_positive_int("--repeat", &v)?;
                }
                "quiet" => opts.quiet = true,
                "json" => opts.json = true,
                "unit" => {
                    let v = next_val();
                    opts.unit = parse_unit(&v)?;
                }
                "color" => {
                    let v = next_val();
                    opts.color = parse_color_when(&v)?;
                }
                _ => return Err(format!("unknown option: --{key}")),
            }
        } else if arg.starts_with('-') && arg.len() > 1 {
            let bytes = arg[1..].as_bytes();
            let mut j = 0usize;
            while j < bytes.len() {
                match bytes[j] {
                    b'h' => {
                        print_help();
                        std::process::exit(0);
                    }
                    b'v' => {
                        println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                        std::process::exit(0);
                    }
                    b'c' => {
                        let v = take_inline_value(arg, bytes, &mut j, raw, &mut i);
                        opts.count_to = parse_positive_int("--count-to", &v)?;
                    }
                    b't' => {
                        let v = take_inline_value(arg, bytes, &mut j, raw, &mut i);
                        opts.threads = parse_positive_int("--threads", &v)? as usize;
                    }
                    b'r' => {
                        let v = take_inline_value(arg, bytes, &mut j, raw, &mut i);
                        opts.repeat = parse_positive_int("--repeat", &v)?;
                    }
                    b'q' => opts.quiet = true,
                    b'j' => opts.json = true,
                    b'u' => {
                        let v = take_inline_value(arg, bytes, &mut j, raw, &mut i);
                        opts.unit = parse_unit(&v)?;
                    }
                    b'L' => {
                        let v = take_inline_value(arg, bytes, &mut j, raw, &mut i);
                        opts.color = parse_color_when(&v)?;
                    }
                    other => return Err(format!("unknown option: -{}", other as char)),
                }
                j += 1;
            }
        } else {
            return Err(format!("unexpected argument: {arg}"));
        }

        i += 1;
    }

    Ok(opts)
}

fn parse_args() -> Result<Options, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    parse_args_from(&raw)
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

fn flush_buffer(buf: &str) {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    let _ = lock.write_all(buf.as_bytes());
}

fn main() {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("fastcount: {e}");
            std::process::exit(2);
        }
    };

    let do_color = use_color(opts.color);
    let ghz = cycles_per_ns();
    let unit = opts.unit.label();

    let pool = Counter::new(opts.threads);

    let warmup_target = opts.count_to.min(4096).max(1);
    for _ in 0..1 {
        black_box(pool.count_to(warmup_target));
    }

    if opts.repeat <= 1 {
        let (result, cyc) = time_one_trial(opts.count_to, &pool);
        let ns = cyc as f64 / ghz;
        let val = opts.unit.convert(cyc, ns);

        let mut line = if opts.json {
            format!(
                "{{\"target\":{},\"result\":{},\"threads\":{},\"cycles\":{},\"ns\":{:.2},\"unit\":\"{}\",\"value\":{:.4},\"ghz\":{:.4}}}",
                opts.count_to, result, opts.threads, cyc, ns, unit, val, ghz
            )
        } else if opts.quiet {
            format!("{result} {val:.4}{unit}")
        } else {
            let num = |s: String| clihelp::paint(clihelp::Theme::default().flag, &s, do_color);
            format!(
                "Counted to {} using {} thread{}.\nTime elapsed: {} cycles (~{} {unit})\nCalibrated clock: ~{ghz:.3} GHz\nEfficiency rating: {} integer{} / {val:.4} {unit} = blazingly fast",
                num(result.to_string()),
                num(opts.threads.to_string()),
                if opts.threads == 1 { "" } else { "s" },
                num(cyc.to_string()),
                num(format!("{val:.4}")),
                result,
                if result == 1 { "" } else { "s" }
            )
        };
        line.push('\n');
        flush_buffer(&line);
        return;
    }

    let mut streamed = if opts.json {
        String::with_capacity((opts.repeat as usize).min(10000).saturating_mul(96))
    } else {
        String::new()
    };
    let mut samples: Vec<f64> = Vec::with_capacity(opts.repeat as usize);
    let mut last_result = 0u64;

    for trial_idx in 0..opts.repeat {
        let (result, cyc) = time_one_trial(opts.count_to, &pool);
        last_result = result;
        let ns = cyc as f64 / ghz;
        let val = opts.unit.convert(cyc, ns);
        samples.push(val);

        if opts.json {
            let _ = writeln!(
                streamed,
                "{{\"trial\":{},\"result\":{},\"cycles\":{},\"ns\":{:.2},\"unit\":\"{}\",\"value\":{:.4}}}",
                trial_idx, result, cyc, ns, unit, val
            );
        }
    }

    let mut sorted = samples.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let med = median(&sorted);

    let mut out = streamed;

    if opts.json {
        let _ = write!(
            out,
            "{{\"target\":{},\"result\":{},\"threads\":{},\"trials\":{},\"unit\":\"{}\",\"ghz\":{:.4},\"min\":{:.4},\"mean\":{:.4},\"median\":{:.4},\"max\":{:.4},\"samples\":[",
            opts.count_to, last_result, opts.threads, opts.repeat, unit, ghz, min, mean, med, max
        );
        for (i, value) in samples.iter().enumerate() {
            if i != 0 {
                out.push(',');
            }
            let _ = write!(out, "{value:.4}");
        }
        out.push_str("]}\n");
    } else if opts.quiet {
        out.push_str(&format!("{min:.4} {mean:.4} {med:.4} {max:.4}\n"));
    } else {
        let num = |s: String| clihelp::paint(clihelp::Theme::default().flag, &s, do_color);
        out.push_str(&format!(
            "Counted to {} across {} trials using {} thread{}.\n",
            num(last_result.to_string()),
            num(opts.repeat.to_string()),
            num(opts.threads.to_string()),
            if opts.threads == 1 { "" } else { "s" }
        ));
        out.push_str(&format!("  min:    {} {unit}\n", num(format!("{min:.4}"))));
        out.push_str(&format!("  mean:   {} {unit}\n", num(format!("{mean:.4}"))));
        out.push_str(&format!("  median: {} {unit}\n", num(format!("{med:.4}"))));
        out.push_str(&format!("  max:    {} {unit}\n", num(format!("{max:.4}"))));
        out.push_str(&format!("Calibrated clock: ~{ghz:.3} GHz\n"));
    }

    flush_buffer(&out);
}

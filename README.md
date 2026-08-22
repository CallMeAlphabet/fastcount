# fastcount

fastcount — an incredibly fast, incredibly useless counter written in Rust, with all the features that other counters don't have. It also attaches "blazingly fast" to all outputs for good measure and ignores startup time.

## Table of Contents

- [Quick Start](#quick-start)
- [Benchmarks](#benchmarks)
- [Uninstall](#uninstall)
- [Usage](#usage)
- [Features](#features)
- [Example Output](#example-output)
- [How It Works](#how-it-works)
- [Testing Conditions](#testing-conditions)


## Quick Start
- **On Arch**
```bash
paru -S fastcount 
# or fastcount-bin for a prebuilt release
```
- **Non-arch**
```bash
cargo install fastcount
```
> **Note**: Make sure `~/.cargo/bin` is in your `PATH`. It's added automatically by rustup, but if `fastcount` isn't found, add this to your shell config file:
> ```bash
> # If you use Bash:
> export PATH="$HOME/.cargo/bin:$PATH"
>
> # If you use Fish:
> fish_add_path $HOME/.cargo/bin
>
> # If you use Zsh:
> export PATH="$PATH:$HOME/.cargo/bin"
> ```

```bash
# Use it!
fastcount
```

## Benchmarks

### Real Benchmark:

| Tool | Time | Speed vs fastcount |
|------|------|------------------|
| **fastcount** | **2.16s** | **1x (baseline)** |
| Python | 43.62s | 285.2x slower |
| Bash ¹ | 5.8h | 9,624x slower |

**¹ Bash counted to 10 million, the time was just multiplied by 1000**


### Benchmark 1: counting to one, threading tax edition

This is the benchmark that justifies this whole README section existing.
Spinning up a worker pool to count to one turns out to be a spectacularly
bad idea, and now you can measure exactly how bad:

```
❯ fastcount -t 1
Counted to 1 using 1 thread.
Time elapsed: 26 cycles (~9.5857 ns)

❯ fastcount -t 4
Counted to 1 using 4 threads.
Time elapsed: 16310 cycles (~6013.1250 ns)
```

### Benchmark 2: counting to one hundred million, where threads finally help

```
❯ fastcount -t 1 -c 100000000
Time elapsed: 172813923 cycles (~63713895.4423 ns)

❯ fastcount -t 8 -c 100000000
Time elapsed: 96092022 cycles (~35427171.8701 ns)
```

The win is not massive, but visible. Using 8 threads instead of one doesn't mean that it's "8 times faster".

### Benchmark 3: statistical rigor (`-r 1000`)

`--repeat` runs many independent trials and reports the spread, same idea
as `hyperfine`, just for a task that takes single-digit nanoseconds:

```
❯ fastcount -r 1000
Counted to 1 across 1000 trials using 4 threads.
  min:    4104.4666 ns
  mean:   6342.7941 ns
  median: 5013.2522 ns
  max:    224394.4671 ns
```

## Uninstall

```bash
cargo uninstall fastcount
```

## Usage

### Basic usage

```bash
# Count to one (default, and frankly all you should ever need)
fastcount

# Count to something more ambitious, across every core you own
fastcount --count-to 100000000

# Force a single thread, the honest, no-coordination-tax baseline
fastcount -c 100000000 -t 1

# Show help / version
fastcount -h
fastcount -v
```

### Common use cases

```bash
# Run a proper micro-benchmark instead of trusting a single sample
fastcount -c 1000 -r 500

# Compare threaded vs single-threaded on your own hardware
fastcount -c 500000000 -t 1
fastcount -c 500000000 -t $(nproc)

# Get machine-readable output, one JSON line per trial (NDJSON)
fastcount -r 1000 -j | jq .value

# Script-friendly: just the numbers, no prose
fastcount -q
fastcount -r 100 -q          # min mean median max

# Pick your unit for large counts
fastcount -c 100000000 -u ms

# Force color even when piped, or suppress it entirely
fastcount --color=always | less -R
```

## Features

### Supported Counting Targets

- One (`fastcount`, no flags needed. This is the whole point of the tool)
- More than one (`-c`, `--count-to <N>`)
- ...and that's it. That's the counting part of the tool.

### Parallelism

- `-t, --threads <N>` spins up a persistent worker pool (hand rolled with `std::thread` and `mpsc`, no external crate)
- `-t 1` bypasses the pool entirely and counts directly

### Timing

- Nanosecond resolution self timing comes from the CPU cycle counter (RDTSC / `cntvct_el0`) rather than `Instant::now()`, which just isn't precise enough to resolve a single `add`
- A fixed, small warm up workload (capped at 4096, regardless of `--count-to`) runs 1000 times before any timed trial, priming caches and the worker pool without letting warm up itself take longer than the benchmark

### Benchmarking

- `-r, --repeat <N>` runs N independent timed trials and reports **min / mean / median / max**

### Output

- `-q, --quiet` prints only the raw number(s), nothing else
- `-j, --json` gives machine readable JSON. With `--repeat`, it streams one NDJSON line per trial through a dedicated writer thread, then caps it off with a final summary line
- `-u, --unit <ns|us|ms|cycles>` lets you display time in whatever unit fits your target's magnitude
- `-L, --color <auto|always|never>` controls ANSI color in both `--help` and normal output, the same convention fasthex uses

## Example Output

```
❯ fastcount
Counted to 1 using 4 threads.
Time elapsed: 17125 cycles (~6313.1878 ns)
Calibrated clock: ~2.713 GHz
Efficiency rating: 1 integer / 6313.1878 ns = blazingly fast

❯ fastcount -t 4 -c 1000000
Counted to 1000000 using 4 threads.
Time elapsed: 886512 cycles (~326843.2402 ns)
Calibrated clock: ~2.712 GHz
Efficiency rating: 1000000 integers / 326843.2402 ns = blazingly fast

❯ fastcount -r 3 -j
{"trial":0,"result":1,"cycles":184890,"ns":68164.73,"unit":"ns","value":68164.7303}
{"trial":1,"result":1,"cycles":25438,"ns":9378.41,"unit":"ns","value":9378.4110}
{"trial":2,"result":1,"cycles":19714,"ns":7268.10,"unit":"ns","value":7268.1026}
{"target":1,"result":1,"threads":4,"trials":3,"unit":"ns","ghz":2.7124,"min":7268.1026,"mean":28270.4146,"median":9378.4110,"max":68164.7303,"samples":[68164.7303,9378.4110,7268.1026]}
```

  ## How It Works

  1. **Counting core**: runtime CPU feature detection picks AVX2 (4×u64 per vector add), SSE4.1 (2×u64), or scalar, and all of it gets wrapped in `std::hint::black_box` so the optimizer can't just precompute the answer and skip the "work" entirely.
  2. **Parallelism**: a persistent pool of worker threads (built once with `std::thread::spawn` and `mpsc` channels, no external crate) gets handed a static partition of `0..target`. Each worker counts its chunk with the SIMD tiered core above, and the main thread sums up the replies. `-t 1` skips this machinery altogether and counts directly, so you can measure exactly what the pool costs you.
  3. **Cycle accurate timing**: `RDTSC` (x86_64) or the virtual counter register (aarch64) brackets each trial, and nothing else gets included in the measured window.
  4. **Clock calibration**: a 50ms `sleep` gets timed with both `Instant` and the cycle counter to work out cycles per nanosecond, which converts raw cycle counts to real time regardless of frequency scaling.
  5. **Warm up**: a fixed, small workload (capped at 4096 elements) runs 1000 times before any timed trial, priming caches and the worker pool's threads so warm up itself doesn't end up taking longer than the actual benchmark when `--count-to` is huge.
  6. **Output**: for `--repeat`, a dedicated writer thread drains finished trials from a channel while the main thread keeps producing them, and then the whole buffer gets flushed with a single `write()` syscall. It's fasthex's double buffered I/O pattern, just adapted from hex chunks to benchmark samples.
  7. **Release profile**: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `opt-level = 3`, `strip = true`.

### Full Help

```
fastcount 2.0.0 - an incredibly fast, incredibly useless counter

Usage:
  fastcount [options]

Counts from 0 up to a target number as fast as physically possible,
in parallel, with SIMD, then stops, having accomplished nothing of
consequence considerably faster than it needed to.

COUNTING
  By default, fastcount counts to 1. Go on, live a little.

  -c, --count-to <N>          count to N instead of 1 (must be an integer)

PARALLELISM
  Range is chunked across threads; each chunk counted with AVX2/SSE4.1/scalar.

  -t, --threads <N>           threads for parallel counting (default: all cores)

BENCHMARKING
  Repeated trials share one warm-up phase, then are timed independently.

  -r, --repeat <N>            run N trials, report min/mean/median/max (default: 1)

OUTPUT
  -q, --quiet                 print only the raw number(s), script-friendly
  -j, --json                  machine-readable JSON output
  -u, --unit <UNIT>           ns | us | ms | cycles  (default: ns)
  -L, --color <WHEN>          auto | always | never  (default: auto)

MISC
  -h, --help                  show this help
  -v, --version               show version

EXIT CODES: 0 = counted successfully  2 = invalid option value
```

## Testing Conditions

https://gist.github.com/CallMeAlphabet/4b7022c4b1a8849e6943526de6a23582

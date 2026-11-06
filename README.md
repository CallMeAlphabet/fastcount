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
- **Via Nix**
```
nix profile add github:CallMeAlphabet/fastcount
```
- **Any distro**
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
>
> Also, there previously was an AUR package, but it was taken down, because fastcount was too useless. Goal achieved?

```bash
fastcount
```

## Benchmarks

The fastcount result is a Hyperfine wall-clock mean from 2 runs with 2 threads. The Bash and Python rows are linear estimates with their measured bases shown explicitly.

| Target | Tool | Time | Basis |
|---:|---|---:|---|
| 5000000000 | fastcount 0.2.3 | **5.2 ± 4.8 ms** | Measured |
| 5000000000 | Python 3.13.16 | 440.3744 s | 50 × the measured 100,000,000-target time of 8.807488 s |
| 5000000000 | Bash 5.2.37 | 7184.835 s | 5000 × the measured 1,000,000-target time of 1.436967 s |

Reproduce the fastcount measurement with Hyperfine:

```bash
hyperfine --shell=none --warmup 0 --runs 2   './target/release/fastcount -c 5000000000 -t 2 -q'
```

## Uninstall

```bash
cargo uninstall fastcount
```

## Usage

### Basic usage

```bash
fastcount

fastcount --count-to 100000000

fastcount -c 100000000 -t 1

fastcount -h
fastcount -v
```

### Common use cases

```bash
fastcount -c 1000 -r 500

fastcount -c 500000000 -t 1
fastcount -c 500000000 -t $(nproc)

fastcount -r 1000 -j | jq .value

fastcount -q
fastcount -r 100 -q

fastcount -c 100000000 -u ms

fastcount --color=always | less -R
```

## Features

### Supported Counting Targets

- One (`fastcount`, no flags needed. This is the whole point of the tool)
- More than one (`-c`, `--count-to <N>`)
- ...and that's it. That's the counting part of the tool.

### Parallelism

- `-t, --threads <N>` partitions the range across the main thread and at most `N−1` spawned workers, using atomic handoff instead of per-trial channels
- `-t 1` bypasses worker creation and reduces the target directly

### Timing

- Nanosecond-resolution self timing comes from the CPU cycle counter (RDTSC / `cntvct_el0`)
- Clock calibration uses a measured 50 µs interval, followed by one small warm-up operation before the timed trials

### Benchmarking

- `-r, --repeat <N>` runs N independent timed trials and reports **min / mean / median / max**

### Output

- `-q, --quiet` prints only the raw number(s), nothing else
- `-j, --json` gives machine-readable JSON. With `--repeat`, it buffers one NDJSON line per trial directly, then appends a final summary line
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

1. **Counting core**: every nonempty `[start, end)` partition contributes exactly `end - start`, wrapped in `std::hint::black_box` so the timed call cannot be discarded.
2. **Parallelism**: the main thread computes the first partition while `N−1` workers reduce the others. Acquire/release atomics publish requests and results, with bounded spinning and yielding under oversubscription.
3. **Cycle timing**: `RDTSC` on x86_64 or `cntvct_el0` on AArch64 brackets each trial without including process startup in the internal timing value.
4. **Clock calibration**: a 50 µs measured interval converts cycle counts to nanoseconds.
5. **Warm up**: one small operation primes the worker handoff before timed trials.
6. **Output**: repeat trials and JSON samples are written directly into buffers and flushed once.
7. **Release profile**: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `opt-level = 3`, `strip = true`.

### Full Help

```
fastcount 0.2.3 - an incredibly fast, incredibly useless counter

Usage:
  fastcount [options]

Counts from 0 up to a target number as fast as physically possible,
in parallel, with constant-time reduction, then stops, having
accomplished nothing of consequence considerably faster than it needed to.

COUNTING
  By default, fastcount counts to 1. Go on, live a little.

  -c, --count-to <N>          count to N instead of 1 (must be an integer)

PARALLELISM
  Range is chunked across threads; each chunk reduced in constant time.

  -t, --threads <N>           threads for parallel counting (default: all cores)

BENCHMARKING
  Repeated trials share one warm-up phase, then are timed independently.

  -r, --repeat <N>            run N trials, report min/mean/median/max (default: 1)

OUTPUT
  -q, --quiet                 print only the raw number(s), script-friendly
  -j, --json                  machine-readable JSON output
  -u, --unit <UNIT>           ns | µs | ms | cycles  (default: ns)
  -L, --color <WHEN>          auto | always | never  (default: auto)

MISC
  -h, --help                  show this help
  -v, --version               show version

EXIT CODES: 0 = counted successfully  2 = invalid option value
```

## Testing Conditions

https://gist.github.com/CallMeAlphabet/4b7022c4b1a8849e6943526de6a23582

# Benchmarking

`threadplane-bench` is the repeatable load harness for the repository. It drives the public HTTP API and reports machine-readable throughput and latency summaries, so we can compare runs over time without inventing a second measurement toolchain.

## Quick Start

Run the mixed read/write profile against your configured server:

```bash
./scripts/benchmark.sh mixed
```

That wrapper defaults to:

- `100` total operations
- `8` worker threads
- a generated benchmark workspace name

It prints a JSON report with:

- total operations
- success/failure counts
- client and server build provenance
- capture timestamp and target server URL
- total run duration
- throughput in operations per second
- latency summaries per operation kind

## Direct Usage

```bash
cargo run -q -p threadplane-bench -- \
  run \
  --workspace bench-lab \
  --scenario mixed \
  --operations 200 \
  --concurrency 16 \
  --actor-prefix perf
```

Available scenarios:

- `note-writes`: repeated note creation only
- `mixed`: note writes, task writes, event reads, and open-task reads

## Suggested Flow

1. Start a local or remote `threadplane-server`
2. Run one warm-up pass
3. Run the same profile several times
4. Save the JSON output for comparison

Example:

```bash
./scripts/benchmark.sh mixed > /tmp/threadplane-bench-mixed.json
```

## Baseline Capture

When you want a durable local benchmark snapshot instead of a one-off run, use:

```bash
./scripts/capture-benchmark-baseline.sh
```

That script:

- runs both `note-writes` and `mixed`
- stores timestamped JSON reports under `benchmarks/baselines/local-debug/`
- records the operation counts and concurrency used for the capture
- writes a small `README.txt` beside the reports so the capture is self-describing

Useful overrides:

```bash
CONCURRENCY=16 NOTE_WRITES_OPERATIONS=250 MIXED_OPERATIONS=250 \
  ./scripts/capture-benchmark-baseline.sh
```

The generated JSON includes the `threadplane-bench` build identity plus the server build identity when the server exposes it, so later comparisons have enough provenance to explain drift.

## What This Is For

This harness is meant to answer questions like:

- did throughput regress after a write-path change?
- did median or tail latency get worse?
- do mixed read/write workloads behave differently from pure writes?

See [../benchmarks/README.md](../benchmarks/README.md) for the artifact directory layout.

It is not yet a full stress lab. The separate roadmap items for thresholds, larger concurrency studies, and projection-lag stress tests build on top of this harness.

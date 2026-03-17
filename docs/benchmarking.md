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

## What This Is For

This harness is meant to answer questions like:

- did throughput regress after a write-path change?
- did median or tail latency get worse?
- do mixed read/write workloads behave differently from pure writes?

It is not yet a full stress lab. The separate roadmap items for baseline capture, thresholds, and projection-lag stress tests build on top of this harness.

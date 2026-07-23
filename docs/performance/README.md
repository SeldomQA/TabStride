# Runtime performance baselines

Run the browser-independent fixed-cost benchmark:

```bash
python3 scripts/benchmark-runtime.py \
  --binary target/release/tabstride \
  --scenario wait-ms \
  --samples 100
```

Run the TodoMVC Snapshot benchmark after starting an attach session on the
TodoMVC page:

```bash
python3 scripts/benchmark-runtime.py \
  --binary target/release/tabstride \
  --scenario todomvc-snapshot \
  --session abcd \
  --samples 100
```

The report includes sample count, P50, P95, P99, minimum, and maximum for the
ordinary one-process-per-command CLI and the persistent Agent WebSocket client.

## 2026-07-22 baseline

For `tool.wait_ms(1ms)`, the persistent client reduced latency by 63.36% at
P50, 64.82% at P95, and 65.75% at P99. Raw data is in
[`baseline-2026-07-22.json`](baseline-2026-07-22.json).

## Five-step Flow baseline

`flow-baseline-2026-07-22.json` measures one `flow.run` containing five sequential
daemon-local 1ms waits (100 measured samples, 5 warmups, release build):

| Transport | P50 | P95 | P99 |
|---|---:|---:|---:|
| Short-lived `tabstride flow run` | 17.339ms | 19.098ms | 19.522ms |
| Persistent `tabstride client` | 12.900ms | 13.312ms | 14.312ms |

This isolates Flow orchestration and transport cost; it is not a substitute for the real Chrome
TodoMVC benchmark. Repeat it with `scripts/benchmark-runtime.py --scenario five-step-flow`.

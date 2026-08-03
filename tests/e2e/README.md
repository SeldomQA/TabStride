# Real Chrome E2E

The Actionability suite exercises the complete production path:

```text
tabstride CLI → serve/IPC → WebSocket → built MV3 extension → chrome.debugger/CDP → real page
```

It launches a disposable Chrome profile, loads the production extension build,
attaches the active fixture tab, and cleans up the session, service, profile,
and browser process when it finishes. It does not use mocked Chrome APIs.

Run the functional suite:

```bash
pnpm e2e:chrome
```

Run the fixed hot-path benchmark and compare its P95 with the checked-in
baseline. A regression greater than 15% fails:

```bash
pnpm perf:chrome
```

Run the complete single-task baseline. It measures both attach and isolated
paths. Attach uses `session start --snapshot → flow → session stop`; isolated
adds one `tab create` request after its merged Agent-tab Snapshot. Each path
assigns one stable `run_id` to every RPC and reports CLI processes, top-level
RPCs, WebSocket Flow steps, merged/standalone Snapshot counts, Full AX calls,
cache hit rates, and cold/warm latency:

```bash
pnpm perf:task
```

The report is written to `artifacts/single-task-performance.json` and follows
`tests/e2e/actionability/task-performance-report.schema.json`. Its P95 is
compared with `task-performance-baseline.json` using the same 15% threshold.

Run both:

```bash
pnpm test:chrome
```

Set `TABSTRIDE_CHROME=/path/to/chrome` when Chrome is not available in a
standard location. Pass `--headed` directly to
`scripts/actionability-chrome-e2e.mjs` for local diagnosis.

The scenario page covers merged attach/isolated Snapshots and cache reuse;
changed/unchanged/unknown page signals; real navigation; Delta fallback and
new-ref reuse; delayed insertion; disabled-to-enabled transitions; continuous
movement; temporary obstruction; DOM replacement; strict multi-match errors;
structured timeout evidence; Flow success/failure/timeout/user Stop Timing;
attach-tab scope isolation; CLI/Flow parity; one persistent adaptive client;
and a deterministic form-creation task.

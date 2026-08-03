# tabstride

Command-line interface and foreground service for [TabStride](https://github.com/Tencent/TabStride).

Install:

```bash
curl -fsSL https://raw.githubusercontent.com/Tencent/TabStride/main/install.sh | sh
```

Start the local service explicitly with visible logs:

```bash
tabstride serve
```

This is the single supported service entrypoint and stays alive until Ctrl+C. Business commands,
`status`, and `doctor` never start a background service.
Business requests log their method, RPC/session/browser identifiers, duration, and outcome without
logging request payloads or page data.

Use `tabstride <business-command> --timing` for the end-to-end timing breakdown: CLI startup,
IPC connect, service queue wait, WebSocket transit, extension dispatch, CDP, and total runtime.
Timings are persisted under the TabStride home and can be queried or exported:

```bash
tabstride metrics summary
tabstride metrics export --out metrics.json
```

Use one stable id across the commands that make up a user task:

```bash
tabstride session start --mode attach --tab active --snapshot --run-id task-001
tabstride flow run task.yaml --session abcd --run-id task-001
tabstride session stop abcd --run-id task-001
tabstride metrics summary --run-id task-001
tabstride metrics summary --flow checkout --step-index 2
```

Task-correlated metrics include CDP and full accessibility-tree call counts
plus Locator, Snapshot, and overlay cache hit rates.

Snapshots cache the full accessibility tree while the document version is unchanged. Use
`tabstride snapshot --session abcd --incremental` to return only changes relative to the previous
compatible snapshot; navigation and DOM mutations invalidate Locator and Snapshot cache entries.

For Agent harnesses that can keep a child process alive, `tabstride client` exposes the core
protocol as newline-delimited JSON over stdin/stdout while reusing one authenticated WebSocket
connection to `tabstride serve`. Run `tabstride client --help` for its request timeout and
transport options.

Choose the path before execution: known steps use one attach + `flow.run` + stop sequence;
adaptive work keeps one `tabstride client` connection from attach through stop. Do not use
`status`, `doctor`, or `browsers` as a normal readiness preflight.

Batch a known sequence into one service request with Flow:

```bash
tabstride flow validate examples/flows/todomvc.yaml
tabstride flow run examples/flows/todomvc.yaml --session abcd --var task="write code"
```

Flow v1 runs `navigate`, `click`, `fill`, `press`, `select`, `wait_for`, `request_help`, `assert`,
`snapshot`, and `wait_ms` through the existing session queue. It stops at the first failure and
reports timing for every started step, including the failed, timed-out, or cancelled step.
Daemon-local `wait_ms` reports `local_us` only; it also preserves daemon-observed duration when
cancellation wins before extension Timing returns. `websocket_us` is transport without extension
execution; `websocket_roundtrip_us` includes it. `extension_us` includes CDP,
`extension_non_cdp_us` excludes accumulated CDP calls, `cdp_us` is the sum of calls, and
`cdp_span_us` is the first-to-last CDP span. Interaction targets share the same strict Locator in CLI and
Flow: exactly one of `ref`, `css`, `role` + `name`, `label`, `placeholder`, `text`, or `testId`,
with optional `exact` for semantic matching. Zero matches enter Auto Wait and eventually return a
structured `timeout` if still absent; multiple matches return `ambiguous_target` immediately.

All four interactions use the extension's shared Actionability Engine before dispatch. Click waits
for visible/stable/enabled/unobscured state, fill requires editable, select requires an enabled
native `<select>`, and targeted press requires focusable. Actionability failures expose
`reason`, `failed_check`, `elapsed_ms`, and `last_state` in JSON errors.

Interactions accept `--page-update none|signal|delta` (default `signal`). Signal reports the
changed/unchanged/unknown state without AX work; none skips post-action observation; delta returns
an incremental Snapshot only when an exact cached baseline exists. Flow action steps use the same
`page_update` field. Handle `full_required`, `delta_unavailable`, or an absent Delta from an old
extension by requesting a normal Snapshot. `document_change_known=false` is unknown, not unchanged,
and also requires a normal Snapshot.

Examples:

```bash
tabstride click --role button --name Save --exact --session abcd --page-update delta
tabstride fill --label Email --value agent@example.com --session abcd
tabstride press Enter --placeholder "Add a task" --session abcd
tabstride select --test-id country --value SG --session abcd
tabstride assert --text "Write code" --exact --visible --session abcd
tabstride assert --css '.todo.completed' --count 3 --session abcd
```

Assertions retry until success or timeout and support visible/hidden, text equals/contains, value
equals, enabled/disabled, checked/unchecked, count, and URL equals/matches. Flow `assert` steps and
the CLI command use the same extension executor.

Failed interactions and assertions expose minimal diagnostics under JSON `data.evidence`: Locator,
match count, Actionability history, last failed check, current URL, Snapshot, PNG Screenshot,
recent Console errors, and Locator/wait/CDP timings. Evidence collection is best-effort and never
replaces the original error.

Documentation: [../../README.md](../../README.md) · [../../docs/architecture.md](../../docs/architecture.md)
· [0.2.0 release notes](../../docs/release-notes-0.2.0.md)

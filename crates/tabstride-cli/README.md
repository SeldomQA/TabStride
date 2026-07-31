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

Snapshots cache the full accessibility tree while the document version is unchanged. Use
`tabstride snapshot --session abcd --incremental` to return only changes relative to the previous
compatible snapshot; navigation and DOM mutations invalidate Locator and Snapshot cache entries.

For Agent harnesses that can keep a child process alive, `tabstride client` exposes the core
protocol as newline-delimited JSON over stdin/stdout while reusing one authenticated WebSocket
connection to `tabstride serve`. Run `tabstride client --help` for its request timeout and
transport options.

Batch a known sequence into one service request with Flow:

```bash
tabstride flow validate examples/flows/todomvc.yaml
tabstride flow run examples/flows/todomvc.yaml --session abcd --var task="write code"
```

Flow v1 runs `navigate`, `click`, `fill`, `press`, `assert`, `snapshot`, and `wait_ms` through the existing
session queue. It stops at the first failure, reports per-step timings, and propagates timeout and
cancel to the active child operation. Interaction targets share the same strict Locator in CLI and
Flow: exactly one of `ref`, `css`, `role` + `name`, `label`, `placeholder`, `text`, or `testId`,
with optional `exact` for semantic matching. Zero matches enter Auto Wait and eventually return a
structured `timeout` if still absent; multiple matches return `ambiguous_target` immediately.

All four interactions use the extension's shared Actionability Engine before dispatch. Click waits
for visible/stable/enabled/unobscured state, fill requires editable, select requires an enabled
native `<select>`, and targeted press requires focusable. Actionability failures expose
`reason`, `failed_check`, `elapsed_ms`, and `last_state` in JSON errors.

Examples:

```bash
tabstride click --role button --name Save --exact --session abcd
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

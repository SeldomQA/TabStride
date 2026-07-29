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

Use `tabstride -v <business-command>` for client-side timing (`cli_startup_us`,
`daemon_check_us`, `ipc_connect_us`, `total_runtime_us`). The service logs the matching
daemon/extension breakdown (`queue_wait_us`, `websocket_us`, `extension_dispatch_us`,
`cdp_us`, `daemon_runtime_us`) at INFO.

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

Documentation: [../../README.md](../../README.md) · [../../docs/architecture.md](../../docs/architecture.md)

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

Flow v1 runs `navigate`, `click`, `fill`, `press`, `snapshot`, and `wait_ms` through the existing
session queue. It stops at the first failure, reports per-step timings, and propagates timeout and
cancel to the active child operation.

Documentation: [../../README.md](../../README.md) · [../../docs/architecture.md](../../docs/architecture.md)

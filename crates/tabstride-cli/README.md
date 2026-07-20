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

Documentation: [../../README.md](../../README.md) · [../../docs/architecture.md](../../docs/architecture.md)

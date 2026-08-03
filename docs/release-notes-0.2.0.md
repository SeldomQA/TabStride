# TabStride 0.2.0 Release Notes

TabStride 0.2.0 establishes the A-plan fast path for taking over the current Chrome tab. The
supported service lifecycle remains explicit: start `tabstride serve`, connect the extension, run
tasks, and stop the foreground service with Ctrl+C.

## Fast execution paths

Use one of two paths and do not run `status`, `doctor`, or `browsers` before a normal task:

- Deterministic task: attach with an optional merged Snapshot, run one Flow, then stop the Session.
- Adaptive task: keep one `tabstride client` connection for attach, decision-point actions,
  Snapshot/Delta updates, and stop.

```bash
tabstride session start --mode attach --tab active --snapshot
tabstride flow run task.yaml --session abcd
tabstride session stop abcd
```

`session start --snapshot` returns the selected tab metadata and initial Snapshot in one request.
Failure to capture the optional Snapshot is reported in-band and does not leak an attach lease.

## New parameters and result fields

- `click`, `fill`, `press`, and `select` accept `--page-update none|signal|delta`.
- Flow interaction steps accept the equivalent `page_update` field.
- Interaction results include `document_changed`, `document_change_known`, `document_version`, and
  optional `snapshot_delta`.
- Flow steps include corrected queue, WebSocket, extension, CDP, and local timing fields on success,
  failure, timeout, and user interruption.
- `tabstride client` provides newline-delimited protocol requests over one authenticated Agent
  WebSocket connection.

Page-change results have three states:

- changed: `document_change_known=true` and `document_changed=true`;
- unchanged: `document_change_known=true` and `document_changed=false`;
- unknown: `document_change_known=false`; obtain a full Snapshot before making a page-state decision.

Snapshot Delta status is `available`, `unchanged`, `full_required`, or `delta_unavailable`. Consume
`available` directly. Request a normal Snapshot after `full_required`, `delta_unavailable`, a
missing Delta from an older extension, or an unknown page-change result.

## Timing definitions

- `queue_us`: wait in the daemon Session dispatch queue.
- `websocket_us`: daemon-to-extension and extension-to-daemon transport, excluding extension work.
- `websocket_roundtrip_us`: daemon send through daemon receive, including extension work.
- `extension_us`: complete extension processing time, including CDP.
- `extension_non_cdp_us`: extension processing excluding accumulated CDP command time.
- `cdp_us`: sum of individual CDP command durations.
- `cdp_span_us`: first CDP start through final CDP finish; may include waits between commands.
- `local_us`: daemon-local Flow work such as `wait_ms`, or daemon-observed step time when user
  cancellation wins before extension Timing returns.

Use global `--json` for the complete structured result. `--format json` is not a supported option.

## Compatibility boundary

Wire protocol remains `1.0`; all A-plan request and response additions are optional. A protocol
version bump is therefore not required.

| Combination | Supported behavior |
|---|---|
| New CLI/daemon + old extension | Core actions work. Merged Snapshot, page-change signal, and Delta may be absent; treat them as unavailable/unknown and request a normal Snapshot. |
| Old CLI/daemon + new extension | Existing commands and results continue to work; old consumers ignore additional result fields. |
| New extension + old daemon | Existing methods work. New Flow fields must not be used until the daemon is upgraded. |
| New CLI + old daemon | Existing commands work where the old daemon accepts the method. `page_update`, merged Snapshot, corrected Flow Timing, and new Flow steps require the new daemon. |

Protocol major mismatch is rejected. A compatible minor mismatch is allowed with version-skew
diagnostics. If a mixed installation behaves unexpectedly, upgrade the CLI/service and extension
together.

## Upgrade

1. Stop the running `tabstride serve` process with Ctrl+C.
2. Install the 0.2.0 CLI and replace/reload the unpacked Chrome extension with the 0.2.0 build.
3. Start `tabstride serve` again.
4. Confirm the extension popup shows connected and matching application/protocol versions.
5. Run a small attach task. Use `tabstride status` or `tabstride doctor` only if that task fails.

Existing TabStride home data and Metrics JSONL files do not require migration.

## Rollback

1. Stop `tabstride serve` and all active Sessions.
2. Reinstall the previous CLI binary and matching extension build.
3. Start the previous service and reload Chrome.

No data rollback is required. New Metrics fields are additive; older readers may ignore them. Do
not use 0.2.0-only Flow fields after rolling the daemon back.

## Release gates

The release requires Rust formatting, Clippy, workspace tests, generated-Schema drift checks,
TypeScript compilation, extension tests/build, Biome, Stylelint, Node script tests, and the A-7 real
Chrome E2E/performance suite. A-8 documentation and compatibility work does not waive the A-7 gate.

The 2026-08-03 release run passed the gate: click P95 118.266ms, Flow click+assert P95
87.954ms, attach complete-task P95 241.461ms, and isolated complete-task P95 314.026ms.
The attach task used exactly three top-level CLI/RPC calls, four WebSocket Flow steps, one merged
Snapshot, no standalone Snapshot request, and one Full AX acquisition.

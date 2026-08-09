# TabStride

TabStride lets AI test the browser tab you already have open.

<p align="center">
  <img src="docs/assets/tabstride-readme-banner.png" alt="TabStride banner" />
</p>

<p align="center">
  <strong>Let AI agents use your browser without interrupting your work.</strong>
</p>

<p align="center">
  English · <a href="README.zh-CN.md">中文</a>
</p>

**TabStride** connects Cursor, Claude Code, Codex, OpenClaw, CodeBuddy, WorkBuddy, Pi, Hermes Agent, and other shell-capable AI agents to your already logged-in browser.

Need the agent to control the current tab in place? Use `tabstride session start --mode attach --tab active`; it creates no window, moves no tab, and leaves sibling tabs inaccessible.


## TabStride Advantages

- **Reuse real login state**: Agents can work with sites you are already signed
  into, without separate test accounts.
- **Two safe modes**: isolated sessions use a separate, visible Agent Window;
  attach sessions lease exactly one explicitly selected existing tab.
- **Support any Agent**: any Agent that can call a shell can use TabStride
  through the `tabstride` CLI, with no lock-in to a specific model, Agent framework, or
  harness.
- **Built-in human-in-loop**: when a task hits captcha, login, confirmation
  dialogs, or other human-only steps, the Agent can ask you to take over and
  then continue afterwards.

## Runtime Environment

TabStride has two local runtime pieces: the `tabstride` CLI/daemon and the browser
extension.

| Runtime | Support |
| --- | --- |
| Operating systems | macOS (Apple Silicon and Intel), Linux (x64 and ARM64), Windows x64 |
| Browsers | Chrome and Microsoft Edge are supported; other Chromium-based browsers are expected to work when they support unpacked Chromium extensions; Firefox is planned |

## Quick Start

<details open>
<summary><b>Install with your Agent (recommended)</b></summary>

<br>

Already using Cursor, Claude Code, Codex, or another shell-capable agent? Just
copy this one line and send it to your agent — it will install the CLI and skill
for you, then walk you through loading the extension:

```text
Set up tabstride on this machine by following https://raw.githubusercontent.com/SeldomQA/TabStride/main/AGENT_INSTALL.md
```

</details>

<details>
<summary><b>Manual install</b></summary>

<br>

Install the CLI, then load the browser extension. The extension is not on the
Chrome Web Store yet — download it from GitHub Releases and load it manually
(see step 2).

#### 1. Install the `tabstride` CLI

**macOS / Linux** (recommended — installs to `~/.local/bin` from GitHub Releases):

```bash
curl -fsSL https://raw.githubusercontent.com/SeldomQA/TabStride/main/install.sh | TABSTRIDE_REPO=SeldomQA/TabStride sh
```

**Windows** (PowerShell — installs to `~/.local/bin`):

```powershell
$env:TABSTRIDE_REPO = "SeldomQA/TabStride"; irm https://raw.githubusercontent.com/SeldomQA/TabStride/main/install.ps1 | iex
```

**Manual download** — pick the archive for your platform from the
[latest CLI release](https://github.com/SeldomQA/TabStride/releases/latest):

| Platform | Asset |
| --- | --- |
| Windows x64 | `tabstride-v<version>-x86_64-pc-windows-msvc.zip` |
| macOS Apple Silicon | `tabstride-v<version>-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `tabstride-v<version>-x86_64-apple-darwin.tar.gz` |
| Linux x64 | `tabstride-v<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux ARM64 | `tabstride-v<version>-aarch64-unknown-linux-musl.tar.gz` |

Unpack the archive and put the `tabstride` binary on your `PATH`. On macOS,
clear the quarantine flag first if Gatekeeper blocks the binary:

```bash
xattr -d com.apple.quarantine tabstride
```

Verify the binary:

```bash
tabstride --version
```

#### 2. Install the browser extension

The extension is not published on the Chrome Web Store yet. Download
`tabstride-extension-v<version>-chrome.zip` from the
[latest extension release](https://github.com/SeldomQA/TabStride/releases/tag/ext-v0.2.0),
then load it manually (the zip is platform-independent and works on Windows,
macOS, and Linux):

1. Unzip the archive to a permanent location.
2. Open `chrome://extensions/` in Chrome or Microsoft Edge.
3. Enable **Developer mode** (toggle in the top-right corner).
4. Click **Load unpacked** and select the unzipped folder.

Because the extension is loaded unpacked, it does not update automatically.
After each new extension release, download the new zip, replace the folder
contents, and click the reload button on the TabStride extension card.

#### 3. Install the skill

TabStride ships a skill that teaches your agent harness how to use `tabstride`. For
these harnesses, install it in one step:

<p align="center">
<table>
  <tr>
    <td align="center" width="108"><a href="https://cursor.com" title="Cursor"><img src="docs/assets/harnesses/cursor.svg" height="36" alt="Cursor" /></a><br /><sub><b>Cursor</b></sub></td>
    <td align="center" width="108"><a href="https://docs.anthropic.com/en/docs/claude-code" title="Claude Code"><img src="docs/assets/harnesses/claude.svg" height="36" alt="Claude Code" /></a><br /><sub><b>Claude Code</b></sub></td>
    <td align="center" width="108"><a href="https://developers.openai.com/codex" title="Codex"><img src="docs/assets/harnesses/codex.svg" height="36" alt="Codex" /></a><br /><sub><b>Codex</b></sub></td>
    <td align="center" width="108"><a href="https://openclaw.ai" title="OpenClaw"><img src="docs/assets/harnesses/openclaw.svg" height="36" alt="OpenClaw" /></a><br /><sub><b>OpenClaw</b></sub></td>
    <td align="center" width="108"><a href="https://www.codebuddy.ai" title="CodeBuddy"><img src="docs/assets/harnesses/codebuddy.svg" height="36" alt="CodeBuddy" /></a><br /><sub><b>CodeBuddy</b></sub></td>
    <td align="center" width="108"><a href="https://www.workbuddy.ai" title="WorkBuddy"><img src="docs/assets/harnesses/workbuddy.svg" height="36" alt="WorkBuddy" /></a><br /><sub><b>WorkBuddy</b></sub></td>
    <td align="center" width="108"><a href="https://github.com/badlogic/pi-mono" title="Pi"><img src="docs/assets/harnesses/pi.svg" height="36" alt="Pi" /></a><br /><sub><b>Pi</b></sub></td>
    <td align="center" width="108"><a href="https://github.com/NousResearch/hermes-agent" title="Hermes Agent"><img src="docs/assets/harnesses/hermes.png" height="36" alt="Hermes Agent" /></a><br /><sub><b>Hermes Agent</b></sub></td>
  </tr>
</table>
</p>

```bash
tabstride install-skill
```

Use <kbd>Space</kbd> to select the Agent harness you want to install into, then
press <kbd>Enter</kbd> to install the skill. Run `tabstride install-skill --list` to see
internal variants and install paths.

Other shell-capable agent harnesses are supported too. Copy
[`skill/SKILL.md`](skill/SKILL.md) into your harness's skills directory as
`tabstride/SKILL.md` to install the skill manually.

</details>

Start a new Agent session and write a prompt that needs the browser, for example:

```text
/tabstride open example.com and summarize what is on the page.
```

### Run the local service in the foreground

Start the TabStride service explicitly before running browser commands:

```bash
tabstride serve
```

This is the single supported service entrypoint. It starts IPC, WebSocket, session management, and
request processing together, and stops them together when you press <kbd>Ctrl</kbd>+<kbd>C</kbd>.
Use `tabstride serve --help` to configure the WebSocket port or session idle timeout.

Run business commands from another terminal. If the service is absent, they fail immediately and
tell you to run `tabstride serve`; they never create a background process. `tabstride status` and
`tabstride doctor` remain read-only diagnostics and never start the service.

### Choose a session mode

TabStride supports two session modes:

- **Isolated (default)** — `tabstride session start` opens a dedicated Agent Window. Use this when
  the agent should work separately from your current browsing.
- **Attach** — `tabstride session start --mode attach --tab active` leases the active tab in your
  current Chrome window in place. You can also target a known tab with `--tab-id <ID>`.

For example, keep `tabstride serve` running in one terminal and run this lifecycle in another:

```bash
session_id=$(tabstride session start --mode attach --tab active)
tabstride snapshot --session "$session_id"
# navigate, click, fill, and other business commands always use the same session id
tabstride session stop "$session_id"
```

Attach mode controls exactly one existing tab. It does not create a window, move the tab, expose
sibling tabs, or permit tab-management commands such as `tab create`, `tab close`, `tab borrow`,
and `tab return`. Stopping the session detaches browser control and removes the control overlay,
while leaving the user's tab and window open. Always stop the session, including after errors.
If the user clicks Chrome's **Cancel** button in the “TabStride started debugging this browser”
banner, TabStride treats it as an explicit revocation: the current command ends with
`user_aborted`, the attach session is released, and the agent must stop immediately. Start a new
attach session only after a new user request; never retry or reattach automatically.

The Chrome extension popup always includes a collapsible **AI operation logs** panel and keeps the
latest 100 operations across sessions. While a session is active, the in-page control overlay also
offers the same live log view. Entries show running/success/failure state, safe target summaries,
and duration. Logs never include fill values, evaluated scripts, page content, or URL query data.

### Locate elements reliably

`click`, `fill`, `press`, and `select` share one strict Locator model. Use one of a snapshot `ref`,
`css`, `role` + accessible `name`, `label`, `placeholder`, visible `text`, or `testId`. Add
`--exact` to semantic locators when substring matching is too broad:

```bash
tabstride click --role button --name Save --exact --session "$session_id"
tabstride fill --label Email --value agent@example.com --session "$session_id"
tabstride press Enter --placeholder "Add a task" --session "$session_id"
tabstride select --test-id country --value SG --session "$session_id"
```

The compatibility positional form remains available for refs and CSS, such as
`tabstride click @e3` or `tabstride click '#submit'`. Every Locator is strict: one match proceeds
and multiple matches return `ambiguous_target` immediately instead of silently using the first
element. Zero matches enter Auto Wait; if the target still has not appeared at the deadline, the
command returns `timeout` with `reason=locator_not_found` (or `ref_not_found`).

Before dispatching an interaction, the extension runs the same Actionability Engine for CLI and
Flow. Click waits for an attached, visible, stable, enabled, event-receiving, unobscured target;
fill additionally requires an editable control; select requires an enabled native `<select>`; and
a targeted press requires a visible focusable element. Each command honours `--timeout`, can be
cancelled, and re-resolves the original strict Locator after DOM mutations, page lifecycle events,
or bounded geometry checks while the target's actionability state changes.
Timeout errors include machine-readable `reason`, `failed_check`, `elapsed_ms`, and `last_state`
fields when using `--json`.

Every `click`, `fill`, `press`, and `select` accepts `--page-update none|signal|delta`:

```bash
tabstride click --role button --name Save --session "$session_id" --page-update delta
```

- `signal` (default) returns `document_changed`, `document_change_known`, and `document_version`
  without fetching the AX tree.
- `none` skips post-action page observation for the lowest overhead.
- `delta` returns `snapshot_delta` when an exact cached Snapshot baseline exists. Its status is
  `available`, `unchanged`, `full_required`, or `delta_unavailable`. An available result includes
  added Snapshot text and `removed_refs`; use a normal Snapshot after `full_required` or
  `delta_unavailable`.

JSON output preserves these structured fields. Human output prints the change state and Delta
status, followed by the incremental Snapshot text when available. An old extension that does not
support Delta is reported as `delta_unavailable` rather than an empty change.

### Assert page state with Auto Wait

`tabstride assert` retries page state until it passes or reaches `--timeout`. Element assertions
support visible/hidden, text equals/contains, value equals, enabled/disabled, checked/unchecked,
and match count. URL assertions support equality and JavaScript regular expressions:

```bash
tabstride assert --text "Write code" --exact --visible --session "$session_id"
tabstride assert --css '.todo.completed' --count 3 --session "$session_id"
tabstride assert --url-matches '/todomvc/#/completed$' --session "$session_id"
```

Each retry re-resolves the original Locator. Element assertions are strict except `count`;
`hidden` also succeeds when the target has no matches. Timeout errors include
`reason=assertion_failed`, `expected`, `actual`, `elapsed_ms`, and `match_count`.

### Inspect minimal failure evidence

Failed interactions and assertions attach a best-effort `data.evidence` object to JSON errors. It
contains the original Locator, match count, every Actionability attempt, the last failed check,
current URL, failure-time accessibility Snapshot and PNG Screenshot, recent Console errors, and
Locator/wait/CDP/evidence timing. Artifact collection never replaces the original error; partial
collection failures are listed in `collection_errors`.

```bash
tabstride --json assert --css '#save' --visible --session "$session_id"
```

The requested `--timeout` remains the browser wait budget. The service allows up to two additional
seconds only after failure so the extension can finish collecting evidence. Cancellation remains
immediate and intentionally skips evidence collection.

### Persistent Agent client

Choose the execution path before starting browser work:

- Deterministic task: attach (optionally with the initial Snapshot) → one `flow.run` → stop.
- Adaptive task: keep one `tabstride client` process alive for attach, decision-point requests,
  Snapshot/Delta updates, and stop.

Do not run status/doctor/browsers as a readiness preflight. A Flow is one Agent request to the
daemon, but the daemon still dispatches one extension WebSocket request per browser Step.

Agent harnesses performing adaptive work should use `tabstride client`. It performs one
authenticated WebSocket handshake with `tabstride serve`, then accepts newline-delimited protocol
requests on stdin and writes correlated responses to stdout:

```text
{"id":"start-1","method":"session.start","params":{"mode":"attach","tab":"active","snapshot":true}}
{"id":"snap-1","method":"tool.snapshot","params":{"session_id":"abcd"}}
{"id":"stop-1","method":"session.stop","params":{"session_id":"abcd"}}
```

Requests may be pipelined and cancelled by request id. The connection sends heartbeats, rejects
duplicate in-flight ids, and cleans up requests and sessions it created when the client disconnects.
The `/agent` endpoint listens only on localhost and requires the random capability stored in the
user-only daemon info file; `tabstride client` handles this handshake automatically.

Consume an available Snapshot Delta directly. Run `tool.snapshot` only for `full_required`,
`delta_unavailable`, a missing Delta from an older extension, or
`document_change_known=false`. A `user_aborted` response ends the task immediately: do not retry
or create a replacement attach session until the user submits a new request.

### Batch repeatable work with Flow

Use Flow when the complete sequence is known up front. The CLI validates a YAML file locally, then
submits every step to the service in one `flow.run` request:

```bash
tabstride flow validate examples/flows/todomvc.yaml
tabstride flow run examples/flows/todomvc.yaml --session "$session_id" --var task="write code"
```

Flow v1 supports `navigate`, `click`, `fill`, `press`, `select`, `wait_for`, `request_help`,
`assert`, `snapshot`, and daemon-side `wait_ms` steps.
Steps run in order through the same session queue as individual CLI commands; the first failure
stops the flow and reports both completed-step timings and a structured `failed_step_result` for
the step that failed, timed out, or was interrupted. Every started browser step includes a `timing`
breakdown; daemon-local steps such as `wait_ms` report `local_us` without fabricated WebSocket/CDP
phases. If cancellation wins before extension Timing returns, `local_us` preserves the duration the
daemon actually observed. Use `--json` to inspect the complete failure payload.
`websocket_us` is request plus response transport only, while `websocket_roundtrip_us` also includes
extension execution. `extension_us` is the extension total and `extension_non_cdp_us` removes the
sum of individual CDP calls. `cdp_us` is that call-by-call sum; `cdp_span_us` is the diagnostic span
from the first CDP start to the last finish and may include waits between calls. A total `timeout` and each
tool's `timeout_ms` are independent, and Ctrl+C cancels the active step and the remaining flow.
Flow targets use the same Locator object and execution path as individual commands. For example,
`target: { role: button, name: Save, exact: true }` has identical matching, errors, scope, and
timeout behavior.

Flow interaction steps also accept the same `page_update` field:

```yaml
- click:
    target: { role: button, name: Save, exact: true }
    page_update: delta
```

Use `none` for predetermined intermediate steps that use stable semantic locators, keep the
default `signal` when only a change decision is needed, and request `delta` only after establishing
a Snapshot baseline when the returned page structure will drive the next decision.

Use `wait_for` for page readiness instead of a fixed delay. It re-resolves the original Locator
until it becomes `attached`, `detached`, `visible`, `hidden`, `enabled`, `disabled`, `editable`,
`checked`, `unchecked`, or `populated`. `request_help` pauses the same Flow for a captcha, login,
or confirmation. Continue resumes the next step; Cancel, timeout, or navigation stops the Flow.
Set the total Flow timeout longer than any human-step timeout.

Inline assertions gate the next step. Top-level `assertions` are final acceptance criteria and run
only after every action step succeeds. Both forms use the same Web-first executor as
`tabstride assert`:

```yaml
steps:
  - wait_for:
      target: { label: Account name }
      state: populated
  - request_help:
      prompt: Complete the confirmation, then choose Continue.
      timeout_ms: 60000
assertions:
  - target: { text: Saved, exact: true }
    visible: true
    timeout_ms: 5000
```

See [`examples/flows/complete-runtime.yaml`](examples/flows/complete-runtime.yaml) for `select`,
`wait_for`, a human step, and final assertions together.

Business requests are logged without their payloads:

```text
INFO request started   rpc_id=nav-a1b2 method="tool.navigate" session="abcd" browser="5301f701"
INFO request completed rpc_id=nav-a1b2 method="tool.navigate" session="abcd" browser="5301f701" duration_ms=119 outcome="ok"
```

Health queries are omitted at INFO level. Form values, page content, selectors, and evaluated
scripts are never included in request logs.
Run `tabstride <business-command> --timing` to print CLI startup, IPC connect, queue wait,
WebSocket, extension dispatch, CDP, and total Runtime in microseconds. Historical timings are
available with `tabstride metrics summary` and `tabstride metrics export --out metrics.json`.
Flow metrics can be isolated by name and step, for example
`tabstride metrics summary --flow checkout --step-index 2`.
For repeated observation, `snapshot --incremental` uses the document-version cache and returns only
accessibility-tree changes.

## How It Works

TabStride is a local bridge between your agent harness and your browser.

```mermaid
flowchart TB
  subgraph Harness["Agent Harness"]
    Agent["Cursor / Claude Code / Codex / OpenClaw"]
  end

  subgraph Local["Your Machine"]
    CLI["tabstride CLI"]
    Daemon["tabstride daemon"]
    Extension["TabStride extension"]
  end

  subgraph Browser["Browser Profile"]
    AgentWindow["Agent Window"]
    UserWindows["Your normal browser windows"]
  end

  Agent -->|"shell: tabstride ..."| CLI
  CLI -->|"local IPC"| Daemon
  Daemon -->|"WebSocket on 127.0.0.1"| Extension
  Extension -->|"isolated: automate"| AgentWindow
  Extension -.->|"attach: lease one tab in place"| UserWindows

  style AgentWindow fill:#fff4e6,stroke:#f59e0b,stroke-width:2px,color:#111827
  style UserWindows fill:#f8fafc,stroke:#cbd5e1,color:#334155
```

The agent never talks to the browser directly. It asks the `tabstride` CLI to perform a
browser task; the local daemon routes that request to the extension; the
extension runs it in an Agent Window by default, or controls one explicitly
leased existing tab in place when the session uses attach mode.

## For Developers

The repository is a Cargo + pnpm workspace:

- `crates/tabstride-cli` — `tabstride` CLI and local daemon
- `crates/tabstride-protocol` — shared wire types and JSON schemas
- `apps/extension` — browser extension
- `packages/ui` and `packages/i18n` — shared extension UI support

Release details, compatibility boundaries, and upgrade/rollback instructions are in the
[0.2.0 release notes](docs/release-notes-0.2.0.md).

## License

MIT

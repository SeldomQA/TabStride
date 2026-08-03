---
name: tabstride
description: |
  Use when the user asks to perform browser automation tasks against their
  logged-in browser: visit and read pages, fill forms, scrape data, click
  through a flow, regression-test a PR's UI, validate a deployed page.
  Requires the tabstride CLI installed and the tabstride extension loaded.
---

# tabstride

Drive the user's **real Chromium browser** (with their logins and cookies) through the `tabstride` CLI. Use an isolated **Agent Window** by default, or use **attach mode** when the user explicitly asks to control one existing tab in place.

## When to use

- Open pages, read titles/text, scrape structured data from sites the user can already access
- Fill forms, click through multi-step flows, smoke-test a UI change
- Understand pages with `tabstride snapshot` first; use `tabstride get-html` or `tabstride screenshot` only when the snapshot is insufficient
- Operate on the user's current tab in place with an explicit attach session

## When NOT to use

- Tasks with **no browser** involved (files, APIs, databases only)
- Installing or configuring the extension (point the user to setup docs instead)
- **Credential harvesting** — never run `tabstride evaluate` on banking, SSO, or password-manager pages to extract tokens, cookies, or secrets
- Long-lived control of a user's personal tab — attach or borrow only for the immediate task, then end the session
- Replacing the user's manual browsing when they only wanted an explanation

## Prerequisites

1. `tabstride` on `PATH` (Rust CLI from tabstride)
2. tabstride **extension** loaded in Chromium and connected (popup shows green)
3. `tabstride serve` running visibly in a separate terminal; business commands never auto-start it

## Fast path: no diagnostic preflight

For a normal automation request, the first service command must be the intended
`tabstride session start`. Do **not** run `tabstride status`, `tabstride doctor`,
`tabstride browsers`, `tabstride session list`, or `tabstride tab list` first to check whether
TabStride is ready. Attempt the requested attach or isolated session directly.

Use the structured `session start` failure instead of a separate discovery request:

- Service not running: ask the user to start `tabstride serve`.
- No browser connected: ask the user to connect the extension, then retry only after confirmation.
- Multiple browsers online: read the candidates included in the error, select one with
  `--browser <instance-id-or-label>` when the user's intent is unambiguous, otherwise ask which
  browser to use. Do not call `tabstride browsers` to retrieve the same list again.
- Unexpected session or browser failure: only then use `status`, followed by `doctor` if the
  status output is insufficient.

Diagnostics are failure recovery tools. Run them before `session start` only when the user
explicitly asked to inspect or diagnose TabStride itself.

## Mandatory workflow

Every automation task **must** follow this lifecycle. Do **not** rely on idle timeouts (default session idle is 5 minutes).
Before the first business command, decide whether the known work can run as a Flow. For any task
with two or more known Flow-supported actions, Flow is the default execution path. If you choose
individual commands instead, state the concrete dependency or unsupported operation that requires
it.

```
1. tabstride session start              → capture the 4-letter session id printed on stdout
2. … every tool command …        → always pass --session <id>
3. tabstride session stop <id>          → REQUIRED when done (even on error paths)
```

Choose the session mode from the user's intent:

- **Isolated (default):** `tabstride session start` opens a dedicated Agent Window.
- **Attach:** `tabstride session start --mode attach --tab active` controls the active tab in the current user window without creating or moving a window/tab. Use `--tab-id <id>` instead of `--tab active` only when the user has identified a specific tab id.

**Merged attach+snapshot:** When the agent needs to immediately understand the page
content after attaching, add `--snapshot` to `session start`:

```
tabstride session start --mode attach --tab active --snapshot
```

This returns the session id AND the initial accessibility snapshot in a single round-trip
(no separate `tabstride snapshot` call needed). The response includes `url`, `title`,
`document_version`, `snapshot_text`, `snapshot_ref_count`, and `snapshot_truncated`.
The snapshot uses the same `@eN` ref system as `tool.snapshot`; refs are immediately usable
in subsequent `click`, `fill`, and other interaction commands.

Optional: after `session start` reports multiple connected browsers, retry with
`--browser <instance-id-or-label>` using a candidate included in that error.

Emergency cleanup: `tabstride session stop --all`.

## Fast execution strategy

Minimize round-trips. A deterministic task on a known page completes in three service calls:

```
1. session start (--mode attach --tab active)   → session id
2. flow run (all actions + assertions)          → one process, one flow.run request
3. session stop                                 → release
```

Add `--snapshot` to step 1 only when the page structure is unknown and you need refs to build
the Flow. If you already know the target locators (CSS, role+name, test-id), skip the snapshot
entirely and let the Flow resolve them at execution time.

**Snapshot minimization:**

| Situation | Action |
|-----------|--------|
| Page structure known from prior context | No snapshot; use stable locators directly |
| Page unknown, need refs for Flow/commands | One `--snapshot` on session start |
| Deterministic intermediate interaction | Set `page_update: none`; use stable semantic locators |
| Adaptive interaction after a Snapshot | Request `page_update: delta`; consume it or follow its fallback status |
| After navigation to a new page | Always re-snapshot (refs are invalidated) |
| Before an assertion on a stable element | No snapshot needed if refs are still valid |

**Anti-patterns (never do these):**

- One CLI process per action for a deterministic sequence → use one Flow.
- Snapshot after every click/fill regardless of `document_changed` → check the flag first.
- Split a Flow to inspect intermediate logs → the Flow result already reports per-step output
  and timing; use the global `--json` flag if needed.
- Run `status`/`doctor`/`browsers` before the first business command → attach directly.
- Repeatedly query the DOM (`get-html`, `evaluate`) to locate an element → use one snapshot
  and semantic locators (`--role`, `--name`, `--label`, `--test-id`).
- Keep a persistent client alive for work that is fully deterministic → use Flow.
- Retry a failed Flow blindly without reading its evidence → inspect `error.data.evidence`.

## Execution path selection

After session start, choose one of two execution paths:

| Criterion | Deterministic (Flow) | Adaptive (persistent client) |
|-----------|---------------------|------------------------------|
| Steps known in advance? | Yes — all actions are predetermined | No — next action depends on page response |
| Intermediate inspection needed? | No | Yes — must read result before deciding next step |
| Typical examples | Fill a known form, create a Todo, run regression steps | Explore an unknown UI, multi-step wizard with branching, debug a failing flow |
| Process model | One `tabstride flow run` process | One long-lived `tabstride client` process |
| Minimum round-trips | 1 (the Flow) | N (one per decision point) |

**Decision rule:** If the task has two or more known Flow-supported actions and later steps do
not depend on inspecting an unknown intermediate result, use Flow. Otherwise use the persistent
client. When only part of a task is deterministic, batch the contiguous known group into a Flow
request through the same persistent client, then continue the adaptive remainder on that client.

Do not spawn one CLI process per action for adaptive work — use the persistent client to keep
one connection alive across multiple request/response cycles.

## Adaptive execution (persistent client)

Use `tabstride client` when the next action genuinely depends on an unknown preceding result.
The client keeps one authenticated WebSocket connection to the running service, accepts one
JSON request frame per stdin line, and writes one correlated response frame per stdout line.

**Lifecycle:**

```
1. Spawn: tabstride client
2. Send session.start request (with snapshot:true for unknown pages)
3. Read response → extract session_id, snapshot_text, document_version
4. Loop:
   a. Send interaction request (tool.click / tool.fill / tool.press / …)
   b. Read response → check snapshot_delta
      - available → use its text and removed_refs as the page update
      - unchanged → keep the current page model
      - full_required / delta_unavailable → send tool.snapshot
      - document_change_known=false (unknown) → send tool.snapshot before deciding the next action
   c. Decide next action from the result
5. Send session.stop request
6. Close stdin (or kill the process) — in-flight work is cancelled
```

**Protocol example (newline-delimited JSON on stdin/stdout):**

```jsonl
→ {"id":"1","method":"session.start","params":{"mode":"attach","tab":"active","snapshot":true}}
← {"id":"1","result":{"session_id":"xkqm","url":"…","snapshot_text":"…","document_version":5,…}}
→ {"id":"2","method":"tool.click","params":{"session_id":"xkqm","target":{"ref":"@e3"},"page_update":"delta"}}
← {"id":"2","result":{"document_changed":true,"snapshot_delta":{"status":"available","text":"…","removed_refs":[]},…}}
→ {"id":"3","method":"tool.fill","params":{"session_id":"xkqm","target":{"label":"Email"},"value":"hello","page_update":"signal"}}
← {"id":"3","result":{"document_changed":false,"document_change_known":true,…}}
→ {"id":"4","method":"session.stop","params":{"session_id":"xkqm"}}
← {"id":"4","result":{}}
```

**Rules:**

- Pipeline requests only when their dependencies allow it; always preserve response IDs.
- Request `delta` only after a Snapshot baseline. Treat `full_required`, `delta_unavailable`, or
  an absent `snapshot_delta` from an old extension as requiring a normal Snapshot.
- Treat `document_change_known=false` as unknown, never as unchanged; refresh with `tool.snapshot`.
- On `user_aborted`, send no more browser requests, do not retry, and do not create another attach
  session. Close the client and wait for a new user request.
- Closing the client cancels in-flight work and stops sessions it created, but still send an
  explicit `session.stop` in the normal success/error cleanup path.
- Do not choose the persistent client over a Flow for a deterministic sequence.

## Flow-first execution

Use one validated Flow instead of separate CLI processes when the task contains two or more known,
Flow-supported actions and later steps do not depend on inspecting an unknown intermediate result:

```
tabstride flow validate <flow.yaml>
tabstride flow run <flow.yaml> --session <id> --var key=value
```

`session start` and `session stop` are lifecycle commands, not Flow steps. For deterministic work,
follow exactly: start one session → validate one Flow → run it once → stop the session.

A Flow is one Agent/CLI request to the daemon, not one extension WebSocket frame. The daemon still
dispatches each browser Step through the session queue and sends one extension request per Step.
Flow removes repeated CLI processes and Agent↔daemon decision round-trips; it does not collapse
multiple browser actions into one browser command.

Use Flow v1 for deterministic `navigate`, `click`, `fill`, `press`, `select`, `wait_for`,
`request_help`, `assert`, `snapshot`, and `wait_ms` steps. Flow and individual commands use the
same strict Locator, Actionability, Auto Wait, cancellation, timeout, and Evidence paths.

Set `page_update: none` on predetermined intermediate actions that use stable semantic locators.
Keep `signal` when only the changed/unchanged decision matters. Use `delta` only after a Snapshot
step when the returned page structure is needed; consume `available`, keep state on `unchanged`,
and run a full Snapshot on `full_required` or `delta_unavailable`. Never request Delta on every
step merely because it exists.

Prefer `wait_for` over `wait_ms` for page readiness. It re-resolves the original Locator and waits
for `attached`, `detached`, `visible`, `hidden`, `enabled`, `disabled`, `editable`, `checked`,
`unchecked`, or `populated`. Use `wait_ms` only when the task requires a real minimum delay rather
than a page state.

Put requested end-state checks in top-level `assertions`; they run only after every action step
succeeds. Use an inline `assert` only when its result must gate a later action.

Every `flow run` result includes per-step `duration_ms` and a `timing` breakdown
(`queue_us`, `websocket_us`, `websocket_roundtrip_us`, `extension_us`,
`extension_non_cdp_us`, `cdp_us`, `cdp_span_us`) when available; daemon-local steps use
`local_us`. Cancellation that wins before extension Timing returns preserves daemon-observed time
in `local_us` without fabricating unavailable transport phases. Use the global `--json` flag to
inspect successful and failed Step timing.

Use `request_help` inside a Flow for a captcha, login, confirmation, or another bounded human step.
Set the Flow's total `timeout` longer than the human step's `timeout_ms`. Continue resumes the Flow;
Cancel, timeout, or navigation stops it and must be treated as a failed/aborted Flow, never as
permission to continue browser operations.

If the initial page state is unknown, run one Snapshot, then build one Flow for all remaining known
actions. Do not keep spawning one CLI process per action after the page is understood. Do not split
a deterministic workflow merely to observe intermediate logs.

Use individual commands only when the next action genuinely depends on an unknown preceding
result or Flow does not support the required operation. Human intervention alone is not a reason
to split the run when `request_help` can express it. When only part of a task is supported, batch
each contiguous supported group into a Flow and run unsupported steps individually.

A Flow stops on its first failure. Inspect its structured evidence and report or diagnose the
failed step. Do not silently retry the whole Flow, skip or weaken a failed assertion, or replace the
Flow with individual commands without first diagnosing why it failed. Ctrl+C cancels the active
step and the rest of the Flow.

For a Flow failure, inspect `error.data.failed_step_result` and
`error.data.cause.data.evidence`; individual command evidence remains at `error.data.evidence`.

## Core interaction loop

Write operations affect only the current session target: an Agent Window tab in isolated mode, or the single leased tab in attach mode.

```
# Fastest path: attach + initial snapshot in one round-trip
tabstride session start --mode attach --tab active --snapshot
# → returns session_id, url, title, document_version, snapshot_text with @eN refs

# Traditional path: separate snapshot after session start
tabstride navigate <url> --session <id>
tabstride snapshot --session <id>          → aria tree with @e1, @e2, … refs
tabstride click @e3 --session <id> --page-update delta
# → available/unchanged, or an explicit full_required/delta_unavailable fallback
tabstride snapshot --session <id>            → only when the Delta result requires it
```

**Refs invalidate after navigation** — always re-snapshot before clicking, filling, or selecting on a new page.

**Choose page observation deliberately:** `--page-update signal` is the CLI default and returns
changed/unchanged/unknown without AX work. Use `none` when no post-action observation is needed.
Use `delta` after a cached Snapshot when the next adaptive decision needs updated structure.
Never interpret `document_changed=false` when `document_change_known=false`; take a Snapshot.
Human output names the state and Delta fallback; `--json` preserves the structured fields.

When an interaction or assertion fails, prefer `--json` and inspect `error.data.evidence` before
retrying. It includes the failure Snapshot, Screenshot, Console errors, Locator match count,
Actionability history, and phase timings. Do not discard or weaken an assertion merely to make it pass.

Prefer `@eN` refs from the latest snapshot. When a stable ref is unavailable, pass exactly one
semantic locator (`--role` + `--name`, `--label`, `--placeholder`, `--text`, or `--test-id`) or
`--css`. Add `--exact` only to semantic locators. A locator must match exactly one element: handle
`not_found` by re-snapshotting or correcting the target, and handle `ambiguous_target` by making the
locator more specific. Never rely on the first match.

## Observation priority

Start with `tabstride snapshot` to understand page structure, text, controls, and element refs. Only escalate when the latest snapshot cannot answer the question:

1. `tabstride snapshot` — default for page understanding and interaction planning
2. `tabstride get-html` — when hidden DOM, metadata, or markup details are required
3. `tabstride screenshot` — when visual layout, canvas/image content, or styling cannot be inferred from the snapshot. Use `--ref @eN` (from the latest snapshot) to crop to one element; omit `--ref` for the full visible tab.

Do **not** call `tabstride get-html` or `tabstride screenshot` first just to inspect a page.

## Sandbox rules

| Rule | Detail |
|------|--------|
| Isolated mode | `tabstride tab create`, `tabstride navigate`, `tabstride click`, etc. work on Agent Window tabs by default |
| Attach mode | Only the explicitly leased existing tab is visible to the session; sibling tabs are inaccessible |
| User tabs in isolated mode | Read-only until borrowed: `tabstride tab list --session <id> --scope user` then `tabstride tab borrow <tab-id> --session <id>` |
| Return borrowed tabs | Call `tabstride tab return <tab-id> --session <id>` when finished; unreturned tabs are **auto-returned** on `tabstride session stop` |
| Tab management in attach mode | `tab create`, `tab close`, `tab borrow`, and `tab return` are unavailable; do not work around this boundary |
| Stop behavior | Isolated stop closes the Agent Window; attach stop releases control and its overlay but keeps the user's tab/window open |
| Chrome debugger Cancel | Treat `user_aborted` as explicit user revocation: stop the task immediately, do not retry, and do not create a replacement attach session until the user makes a new request |

## Global flags

| Flag | Purpose |
|------|---------|
| `--json` | Machine-readable JSON on stdout (errors too) |
| `--quiet` | Suppress informational stderr |
| `-v` / `-vv` | More verbose logging |

Command-specific flags (timeouts, `--tab-id`, `--wait-until`, …): **`tabstride <cmd> --help`**

## CLI command reference (one line each)

Details and flags: **`tabstride <cmd> --help`**

### Failure-only diagnostics

Do not use these commands as readiness checks for a normal browser task. Use them after a failed
business/session request, or when the user explicitly requests diagnostics.

| Command | Summary |
|---------|---------|
| `tabstride status` | Connection health, connected browsers, active sessions |
| `tabstride doctor` | Deep diagnostics and repair hints |
| `tabstride browsers` | List connected browser instances (ids, labels, versions) |

### Flow

| Command | Summary |
|---------|---------|
| `tabstride flow validate <file>` | Validate Flow YAML without contacting the service |
| `tabstride flow run <file> --session <id>` | Submit all steps in one request; repeat `--var key=value` for variables |

### Session

| Command | Summary |
|---------|---------|
| `tabstride session start` | Start an isolated Agent Window session; prints **4-letter session id** |
| `tabstride session start --mode attach --tab active` | Lease the current active user tab in place; `--tab-id <id>` targets a known tab id |
| `tabstride session start --mode attach --tab active --snapshot` | Lease + capture initial page snapshot in one round-trip (A-2) |
| `tabstride session stop <id>` | End session; close isolated window or release attach tab; auto-return borrowed tabs |
| `tabstride session stop --all` | Stop every active session |
| `tabstride session list` | List active sessions |

### Tabs (require `--session <id>`)

| Command | Summary |
|---------|---------|
| `tabstride tab list` | List tabs (`--scope user\|agent\|all`, default `all`) |
| `tabstride tab create` | New tab in Agent Window (`--url`, `--no-active`, `--index`) |
| `tabstride tab close <tab-id>` | Close an agent tab |
| `tabstride tab select <tab-id>` | Focus an agent tab |
| `tabstride tab borrow <tab-id>` | Move a user tab into the Agent Window |
| `tabstride tab return <tab-id>` | Return a borrowed tab to its original window |

### Observation (require `--session` unless noted)

| Command | Summary |
|---------|---------|
| `tabstride snapshot` | First-choice page understanding: accessibility tree with `@eN` element refs |
| `tabstride get-html` | Raw HTML dump after snapshot is insufficient (high token cost) |
| `tabstride screenshot` | PNG capture after snapshot is insufficient: full visible tab, or `--ref @eN` to crop to one element (`--out` path optional) |

### Navigation

| Command | Summary |
|---------|---------|
| `tabstride navigate <url>` | Go to URL in agent tab (`--wait-until`, `--timeout`) |
| `tabstride navigate-back` | History back one step |
| `tabstride navigate-forward` | History forward one step |
| `tabstride reload` | Reload current tab (`--hard` bypass cache) |

(`tabstride navigate back` / `tabstride navigate forward` are equivalent subcommands.)

### Interaction

| Command | Summary |
|---------|---------|
| `tabstride click <ref-or-css>` | Click one strict target; also accepts semantic Locator flags (`--button`, `--click-count`, `--modifiers`) |
| `tabstride fill <ref-or-css> --value <text>` | Clear and type into one strict target; also accepts semantic Locator flags |
| `tabstride select <ref-or-css> --value <v>` | Set one strict `<select>` target by `value`; repeat `--value` for multi-select |
| `tabstride press <key>` | Key/combo (`Enter`, `Ctrl+A`, …); optional ref, CSS, or semantic Locator focuses one target first |
| `tabstride assert` | Web-first assertion with Auto Wait; supports element state/count and URL equality/regex |

All four interaction commands accept `--page-update none|signal|delta` (default `signal`).

Locator examples:

```
tabstride click --role button --name Save --exact --session <id>
tabstride fill --label Email --value agent@example.com --session <id>
tabstride assert --text "Write code" --exact --visible --session <id>
tabstride assert --css '.todo.completed' --count 3 --session <id>
tabstride press Enter --placeholder "Add a task" --session <id>
tabstride select --test-id country --value SG --session <id>
```

### Scripting & timing

| Command | Summary |
|---------|---------|
| `tabstride evaluate <expression>` | Run JS in agent tab (see red lines); JS throw → stderr, **exit 0** |
| `tabstride wait-for-navigation` | Block until load/DOM idle/etc. (`--wait-until`, `--timeout`) |
| `tabstride wait-ms <duration>` | Sleep (`500ms`, `2s`, `1m`; **no** `--session`) |

### Ask the human for help — `tabstride request-help`

When a step needs a human (captcha, login, OTP) or you want the user to
confirm an important action, pause and ask:

    tabstride request-help --session <id> --prompt "Solve the captcha, then click Continue" \
      --title "Captcha required" --target @e7 --target "#submit" --timeout 5m

- `--prompt` (required): what the user should do.
- `--title` (optional): custom title for the overlay panel. When omitted,
  the extension shows its default localized title.
- `--target` (repeatable): a snapshot ref (`@e7`) or CSS selector
  (`#submit`) to scroll to and flash-highlight. **Strongly recommended** —
  whenever the prompt refers to a concrete element (a button to click, a
  field to fill, a checkbox to toggle), pass its `@eN` ref / selector so the
  user is guided straight to the right spot instead of hunting for it. For
  interaction scenarios, always include the relevant target(s); reserve a
  prompt with no `--target` for cases where there is genuinely no specific
  element to point at (e.g. "wait for the page to finish loading").
- `--timeout` (default `5m`): how long to wait.

The target tab is brought to the foreground; the page stays interactive
while the agent control mask is hidden. The call blocks until the user
acts. The result `outcome` is one of:

- `continued` — the user finished and clicked Continue (treat as confirm).
- `cancelled` — the user clicked Cancel (treat as reject/abort).
- `timed_out` — nobody acted within the timeout.
- `navigated` — the page navigated while waiting (full reload or SPA URL change). Snapshot refs are stale; run `tabstride snapshot` on the new page, then decide whether to call `tabstride request-help` again.

`note` carries any text the user typed back. `resolved_targets` reports
which refs/selectors matched a live element.

## Error handling

### Exit codes (`echo $?` after `tabstride …`)

| Code | Meaning | What to do |
|------|---------|------------|
| `0` | Success (including `evaluate` where JS threw but RPC succeeded) | Continue |
| `1` | User error — bad args, unknown session, target outside session scope, stale ref | Fix args; `tabstride session list`; re-snapshot |
| `2` | Protocol / transport — service unreachable, IPC failure | `tabstride doctor`; check extension connected; retry the command |
| `3` | Browser / CDP execution failed | Retry; simplify selector; check tab still open |
| `4` | Timeout | Increase `--timeout`; try `--wait-until domcontentloaded` |
| `5` | Version skew (CLI vs extension) | Upgrade/reinstall matching versions |

Human errors print `error:` + `hint:` on stderr; `--json` includes `code`, `message`, `hint`, `exit_code`.

### When to run diagnostics

| Situation | Command |
|-----------|---------|
| `session start` failed unexpectedly | `tabstride status`; use `doctor` only if status is insufficient |
| A transport/version failure remains unexplained | `tabstride doctor` |
| Multiple browsers reported by `session start` | Reuse its candidate list with `--browser <instance-id-or-label>`; do not call `browsers` again |

`user_aborted` is not a retryable protocol failure even though it shares exit code 2: stop
immediately and wait for a new user request.

Always **`tabstride session stop <id>`** in a `finally`-style path so the Agent Window closes or the attach lease and control overlay are released, and borrowed tabs return.

## Red lines

1. **No token theft** — do not `tabstride evaluate` on sensitive sites to read `localStorage`, cookies, or auth headers for exfiltration.
2. **No long control** — do not leave a user's personal tab attached or borrowed across unrelated tasks.
3. **No skip stop** — always `tabstride session stop <id>`; never assume idle timeout will clean up.
4. **No observe escalation before snapshot** — use `tabstride snapshot` first; only use `tabstride get-html` or `tabstride screenshot` when the snapshot is insufficient. Element screenshots (`--ref @eN`) still require a fresh snapshot ref — never skip snapshot just to grab a visual.
5. **`evaluate` is powerful and risky** — use only when snapshot + click/fill/select cannot suffice; never on credential surfaces.

---

**More detail for any command:** `tabstride <cmd> --help`

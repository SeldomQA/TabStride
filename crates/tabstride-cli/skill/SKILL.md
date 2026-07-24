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

## Mandatory workflow

Every automation task **must** follow this lifecycle. Do **not** rely on idle timeouts (default session idle is 5 minutes).

```
1. tabstride session start              → capture the 4-letter session id printed on stdout
2. … every tool command …        → always pass --session <id>
3. tabstride session stop <id>          → REQUIRED when done (even on error paths)
```

Choose the session mode from the user's intent:

- **Isolated (default):** `tabstride session start` opens a dedicated Agent Window.
- **Attach:** `tabstride session start --mode attach --tab active` controls the active tab in the current user window without creating or moving a window/tab. Use `--tab-id <id>` instead of `--tab active` only when the user has identified a specific tab id.

Optional: add `--browser <instance-id>` to either session-start mode when multiple browsers are connected (`tabstride browsers` / error output lists their ids).

Emergency cleanup: `tabstride session stop --all`.

## Persistent client

When the harness can keep a child process alive, prefer `tabstride client` over spawning one CLI
process per browser step. It accepts one protocol request per stdin line, writes one correlated
response per stdout line, and keeps a single authenticated WebSocket connection to the running
service. Pipeline requests only when their dependencies allow it; always preserve response IDs.
Closing the client cancels its in-flight work and stops sessions it created, but still send an
explicit `session.stop` in the normal success/error cleanup path.

## Flow batching

When all steps are known before execution, prefer one validated Flow over separate CLI processes:

```
tabstride flow validate <flow.yaml>
tabstride flow run <flow.yaml> --session <id> --var key=value
```

Use Flow v1 only for deterministic `navigate`, `click`, `fill`, `press`, `snapshot`, and `wait_ms`
steps. Flow and individual commands use the same strict Locator and execution path. A flow stops on
its first failure; do not rewrite or skip the failed step silently. Ctrl+C cancels the active step
and the rest of the flow. Continue to use individual commands when the next action depends on
inspecting the preceding result.

## Core interaction loop

Write operations affect only the current session target: an Agent Window tab in isolated mode, or the single leased tab in attach mode.

```
tabstride navigate <url> --session <id>
tabstride snapshot --session <id>          → aria tree with @e1, @e2, … refs
tabstride click @e3 --session <id>          → or tabstride fill, tabstride select, tabstride press
tabstride snapshot --session <id>            → again after navigation / DOM change
```

**Refs invalidate after navigation** — always re-snapshot before clicking, filling, or selecting on a new page.

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

## Global flags

| Flag | Purpose |
|------|---------|
| `--json` | Machine-readable JSON on stdout (errors too) |
| `--quiet` | Suppress informational stderr |
| `-v` / `-vv` | More verbose logging |

Command-specific flags (timeouts, `--tab-id`, `--wait-until`, …): **`tabstride <cmd> --help`**

## CLI command reference (one line each)

Details and flags: **`tabstride <cmd> --help`**

### Diagnostics

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

Locator examples:

```
tabstride click --role button --name Save --exact --session <id>
tabstride fill --label Email --value agent@example.com --session <id>
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
| Before first task in a session | `tabstride status` — extension connected? |
| Any failure you cannot fix in one retry | `tabstride doctor` |
| Multiple browsers / wrong target | `tabstride browsers` then add `--browser <instance-id>` to the isolated or attach start command |

Always **`tabstride session stop <id>`** in a `finally`-style path so the Agent Window closes or the attach lease and control overlay are released, and borrowed tabs return.

## Red lines

1. **No token theft** — do not `tabstride evaluate` on sensitive sites to read `localStorage`, cookies, or auth headers for exfiltration.
2. **No long control** — do not leave a user's personal tab attached or borrowed across unrelated tasks.
3. **No skip stop** — always `tabstride session stop <id>`; never assume idle timeout will clean up.
4. **No observe escalation before snapshot** — use `tabstride snapshot` first; only use `tabstride get-html` or `tabstride screenshot` when the snapshot is insufficient. Element screenshots (`--ref @eN`) still require a fresh snapshot ref — never skip snapshot just to grab a visual.
5. **`evaluate` is powerful and risky** — use only when snapshot + click/fill/select cannot suffice; never on credential surfaces.

---

**More detail for any command:** `tabstride <cmd> --help`

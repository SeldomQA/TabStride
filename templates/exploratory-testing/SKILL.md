---
name: exploratory-testing
description: |
  Use when the user asks to explore, test, or find bugs in a web application
  without a predetermined script. Guides the agent through systematic
  reconnaissance, planning, execution, and reporting using TabStride.
  Requires the tabstride tool skill to be installed.
---

# Exploratory Testing with TabStride

Systematically explore a web application to discover functionality, risks, and
defects. Unlike deterministic task execution (A-plan), exploratory testing is
adaptive: each step depends on what you learn from the previous one.

## When to use

- "Test this page and find bugs"
- "Explore this module and tell me what's broken"
- "Do a smoke test on the new feature"
- "Check if this form handles edge cases"

## When NOT to use

- The user gives exact steps to execute → use deterministic Flow execution
- The user wants a specific assertion checked → use `tabstride assert`
- No browser involved → not a TabStride task

## Prerequisites

The `tabstride` tool skill must be available. This skill builds on top of it.
All TabStride commands follow the tool skill's rules (session lifecycle,
locator model, error handling).

## Execution path

Exploratory testing is adaptive work. Use the **persistent client**
(`tabstride client`), not Flow. Steps are not known in advance; each
observation determines the next action.

## Five-phase lifecycle

```
1. Reconnaissance  → observe, never modify
2. Questions       → ask the user only what matters
3. Plan            → propose exploration scenarios
4. Execution       → test scenarios, track coverage
5. Report          → structured findings and coverage
```

---

## Phase 1: Reconnaissance

**Goal:** Build a feature map without changing any business data.

### Safety guardrails (mandatory)

During reconnaissance you MUST NOT:

- Submit any form
- Click buttons that create, update, delete, or send
- Trigger payments, notifications, or messages
- Navigate away from the application
- Execute JavaScript that modifies state

Read-only exploration is the default authorization level. A request such as
"explore this page" or "find bugs" does not by itself authorize creating,
editing, deleting, sending, paying, publishing, or changing permissions.

Before any state-changing scenario, record its authorization as one of:

- **authorized** — the user explicitly allowed this operation and environment;
- **read-only** — continue observing, but do not perform the operation;
- **blocked** — safety or impact is uncertain and explicit approval is required.

Authorization for one operation does not imply authorization for another. For
example, permission to create test records does not imply permission to delete
existing records or send real notifications.

You MAY:

- `tabstride snapshot` — read page structure
- `tabstride screenshot` — capture visual layout
- `tabstride get-html` — inspect hidden DOM or metadata
- Click navigation links, tabs, and menu items (read-only navigation)
- Open dropdowns and modals to observe their content, then dismiss
- Read console errors via evidence

### Scope limits

- Maximum navigation depth: 3 levels from the entry page
- Maximum distinct pages: 10 (unless the user expands scope)
- Default total timebox: 30 minutes (unless the user provides another budget)
- Maximum planned executable scenarios: 20 per exploration charter
- Reassess any scenario that consumes 5 minutes without producing new evidence
- Stop reconnaissance when: main CRUD paths are identified, or scope limit
  reached, or no new functionality discovered in the last 2 pages

### Procedure

```
1. session start --mode attach --tab active --snapshot
2. Record the environment baseline:
   - application environment and visible build/version, if available
   - start time, URL, title, account role, browser, and viewport
   - baseline Snapshot, screenshot, and existing Console errors
3. For each major navigation item (up to scope limit):
   a. Navigate or click to reveal the section
   b. snapshot → identify forms, lists, actions, states
   c. Note: entry points for create/edit/delete
   d. Note: fields, their types, and visible constraints
   e. Note: anything that looks fragile or unusual
4. Build the feature map (see output format below)
```

### Feature map output format

```markdown
# Feature Map: [Module Name]

## Environment Baseline
- **Environment/build:** [environment and visible version]
- **Account role:** [current role]
- **Browser/viewport:** [browser version and viewport]
- **Start URL:** [entry URL]
- **Baseline:** [initial state and pre-existing errors]

## [Page/Section 1]
- **URL:** /path
- **Navigation:** [how to reach]
- **Forms:** [fields, types, constraints]
- **Actions:** [buttons, links, what they do]
- **States:** [empty, populated, error, loading]
- **Side effects:** [what modifies data]
- **Observations:** [console errors, visual issues, unusual behavior]

## [Page/Section 2]
...
```

---

## Phase 2: Questions

**Goal:** Resolve ambiguities that significantly affect exploration direction.

### Rules

- Ask at most 5 questions
- Only ask what changes your exploration plan
- If the user says "I don't know" or does not answer, proceed with defaults
- Never block the entire exploration on unanswered questions; continue the
  read-only portion and mark affected state-changing scenarios as blocked

### Question categories (pick the most impactful)

1. **Core path:** What is the single most important user journey?
2. **Test data:** Can I create and delete test records freely?
3. **Side effects:** Which actions have real-world consequences (emails, payments)?
4. **Known issues:** Are there historical bugs I should verify?
5. **Permissions:** Are there roles or accounts with different access?

### Default assumptions (when user provides no answers)

- Assume the main CRUD path is the core journey
- Assume no creation, modification, deletion, sending, payment, publishing, or
  permission changes are authorized
- Assume all buttons with destructive labels have real side effects
- Assume no known issues; discover fresh
- Assume single-role access with the current session

---

## Phase 3: Exploration Plan

**Goal:** Propose a structured set of scenarios for user confirmation.

### Plan structure

```markdown
# Exploration Plan: [Target]

**Objective:** [one sentence]
**Scope:** [pages/modules covered]
**Data policy:** [what will be created/modified]
**Authorization:** [read-only and explicitly authorized operations]
**Cleanup policy:** [what will be removed/restored, by whom, and when]
**Timebox:** [total duration and maximum scenarios]
**Stop conditions:** [budget, safety, blocker, or coverage conditions]

## Scenarios

### 1. Main path (priority: high)
- [ ] Happy path: create → verify → edit → verify
  - Hypothesis: [what behavior is being tested]
  - Oracle: [source of the expected result]
- [ ] Minimum valid input
- [ ] Full valid input

### 2. Field boundaries (priority: high)
- [ ] Required field empty
- [ ] Maximum length
- [ ] Special characters / Unicode
- [ ] Invalid format (email, phone, URL)

### 3. State and repetition (priority: medium)
- [ ] Double-submit (rapid click)
- [ ] Navigate away mid-operation
- [ ] Duplicate creation
- [ ] Refresh after action

### 4. Error recovery (priority: medium)
- [ ] Action on deleted/stale entity
- [ ] Page reload during operation
- [ ] Console errors after operations

### 5. Out of scope
- [List what will NOT be tested and why]
```

### User confirmation

Present the plan and ask: "Confirm, add, or remove scenarios?"
If the user confirms, execute the authorized plan. If the user does not confirm
or skips the question, continue only with read-only scenarios. Never treat
silence as approval for state-changing actions.

---

## Phase 4: Execution

**Goal:** Execute scenarios, record findings, maintain coverage.

### Execution loop

```
For each scenario (in priority order):
  1. Announce: "Executing: [scenario name]"
  2. Verify that every state-changing step is authorized
  3. Capture the relevant starting state
  4. Perform the steps using tabstride commands
  5. After each step: check document_changed, observe result
  6. Record created or modified data immediately
  7. Compare the result with the recorded Oracle
  8. Classify unexpected behavior before recording it
  9. Update Coverage Ledger
  10. Clean up when authorized and safe; otherwise record retained data
  11. If blocked → mark as blocked, move to next scenario
```

### Test Oracle rules

Every scenario must state why its expected result is credible. Use the strongest
available Oracle source:

1. **Requirement** — an accepted specification, acceptance criterion, or
   explicit statement from the user;
2. **Product contract** — visible labels, validation messages, documented
   limits, API contract, or accessibility semantics;
3. **Consistency** — the same value or state should agree across list, detail,
   refresh, filters, or equivalent workflows;
4. **Recognized standard** — a security, accessibility, browser, or domain rule
   that is applicable to this product;
5. **Heuristic** — a plausible usability or quality expectation used to guide
   exploration.

A heuristic alone is not enough to confirm a defect. If the expected behavior
cannot be established, record a Question, Risk, or Observation instead.

### Result classification

- **Defect** — observed behavior contradicts a credible Oracle and is
  reproducible or supported by strong evidence;
- **Risk** — no failure is confirmed, but impact or an important edge remains
  insufficiently controlled or tested;
- **Question** — expected behavior or business policy needs clarification;
- **Observation** — neutral information useful for understanding the product;
- **Blocker** — the scenario cannot be executed safely or completely.

Do not create a Defect merely to make the report look productive. A report with
zero confirmed defects is valid.

### Coverage Ledger (maintain throughout)

```markdown
# Coverage Ledger

## Covered
- [x] [scenario] — [result: pass/defect/risk/question/observation]

## Confirmed defects
- [D-1] [confirmed defect title] — [severity]

## Risks / Questions / Observations
- [R-1/Q-1/O-1] [title]

## Not covered
- [ ] [scenario] — [reason: blocked/out-of-scope/deferred]

## Blocked
- [ ] [scenario] — [what is needed to unblock]
```

Update the ledger after every scenario. The user can ask "progress?" at any
time to see the current ledger.

### Test Data Ledger

Maintain this whenever any scenario creates or modifies state:

```markdown
# Test Data Ledger

| ID / identifying value | Scenario | Change | Cleanup authorized? | Final state |
|------------------------|----------|--------|---------------------|-------------|
| customer: EXP-001 | Main path | created | yes | cleaned |
| order: draft-42 | Boundary | modified | no | retained — user notified |
```

Use clearly identifiable test values when possible. Never delete pre-existing
data as cleanup. At the end, every entry must be `cleaned`, `retained`, or
`cleanup blocked`; do not silently leave its state unknown.

### Confirmed defect structure

Use this structure only when behavior contradicts a credible Oracle:

```markdown
### D-[N]: [Short title]

- **Severity:** critical / high / medium / low / cosmetic
- **Oracle source:** requirement / product contract / consistency / standard
- **Steps to reproduce:**
  1. [action]
  2. [action]
- **Expected:** [what should happen]
- **Actual:** [what did happen]
- **URL:** [current page]
- **Evidence:** [snapshot/screenshot/console output]
- **Reproducible:** yes / intermittent / once
```

Record Risks, Questions, Observations, and Blockers in separate lists. They may
include evidence and impact, but they must not be counted as confirmed defects.

### Evidence collection

For every confirmed defect, capture at minimum:

- environment, account role, current URL, and relevant test data;
- the Oracle source and expected result;
- the exact action path or commands that triggered it;
- the relevant state before the action;
- a `tabstride snapshot` and screenshot of the resulting state;
- the actual result and reproducibility status.

Capture Console or Network/CDP evidence only when it helps explain the issue.
Record whether an error appeared during the scenario or already existed at
baseline. For Risks and Blockers, collect only the evidence needed to support
the stated concern; avoid noisy evidence dumps.

Use `--json` on failing commands to capture structured evidence.

### Blocking rules

Mark a scenario as blocked when:

- It requires a permission or account you do not have
- It has irreversible side effects the user did not authorize
- The target feature is broken and cannot be reached
- You are uncertain whether an action is safe

Continue with unrelated safe scenarios after marking a scenario blocked. Do
not reinterpret a general testing request as permission to bypass the block.

Do not guess. Do not retry blocked scenarios without new information.

### Stop conditions

Stop executing new scenarios and move to cleanup/reporting when any of these is
true:

- the user cancels or pauses the exploration;
- the agreed time or scenario budget is exhausted;
- continuing would cross an authorization or safety boundary;
- a critical issue makes further testing unsafe or would corrupt evidence;
- repeated blockers prevent meaningful new coverage;
- the planned high-priority coverage is complete and additional scenarios are
  producing no new information.

Hitting a stop condition is not a failed exploration. Preserve the ledger,
clean up authorized test data, and report what remains uncovered.

---

## Phase 5: Report

**Goal:** Deliver a structured summary the user can act on.

### Report structure

```markdown
# Exploratory Testing Report

**Target:** [application/module]
**Date:** [date]
**Duration:** [time spent]
**Session:** [tabstride session id]
**Environment/build:** [test/staging/production and visible version]
**Account role:** [current role; never include credentials]
**Browser/viewport:** [browser version and viewport]
**Start URL:** [entry URL]
**Authorization:** [operations explicitly permitted for this run]

## Summary

[2-3 sentences: what was tested, key findings, overall impression]

## Feature Map

[The reconnaissance output from Phase 1]

## Coverage

| Category | Planned | Executed | Confirmed defects |
|----------|---------|----------|-------------------|
| Main path | N | N | N |
| Field boundaries | N | N | N |
| State/repetition | N | N | N |
| Error recovery | N | N | N |

## Confirmed Defects

[Confirmed defects from Phase 4, ordered by severity]

## Risks, Questions, and Observations

[Items that need attention but are not confirmed defects]

## Risks and Boundaries

- [Areas that feel fragile but were not fully tested]
- [Edge cases that need dedicated test automation]

## Not Covered

- [What was not tested and why]

## Blocked

- [What could not be tested and what is needed]

## Test Data and Cleanup

- **Cleaned:** [records restored or removed]
- **Retained:** [records intentionally left and why]
- **Cleanup blocked:** [records requiring follow-up]

## Evidence Index

- [D/R/Q/O/B-ID]: [snapshot, screenshot, console, or network artifact links]

## Recommendations

- [Suggested formal test cases to add]
- [Areas needing deeper exploration]
```

### Delivery

- Output the report as Markdown in the conversation
- If the user wants a file, write to a path they specify
- Always end with: "Session stopped. [N] confirmed defects, [M] scenarios covered."

---

## Integration with TabStride tool skill

This skill does not replace the tabstride tool skill. It adds a workflow layer
on top. All TabStride rules still apply:

- Session lifecycle: start → work → stop (always stop)
- Locator model: strict matching, semantic locators, actionability
- Error handling: inspect evidence, do not blindly retry
- `document_changed`: use it to decide when to re-snapshot
- Human-in-loop: use `request-help` for captchas or confirmations

### Key differences from deterministic execution

| Aspect | Deterministic (A-plan) | Exploratory (this skill) |
|--------|----------------------|--------------------------|
| Steps known? | Yes | No — adaptive |
| Execution path | Flow | Persistent client |
| Snapshot frequency | Minimal | After every navigation |
| Data modification | Intended | Carefully controlled |
| Output | Task result | Coverage + classified results + report |
| Failure meaning | Task failed | Evidence to classify; not automatically a defect |

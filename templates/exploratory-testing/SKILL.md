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
- Stop reconnaissance when: main CRUD paths are identified, or scope limit
  reached, or no new functionality discovered in the last 2 pages

### Procedure

```
1. session start --mode attach --tab active --snapshot
2. Record: URL, title, visible navigation elements
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
- Never block on unanswered questions

### Question categories (pick the most impactful)

1. **Core path:** What is the single most important user journey?
2. **Test data:** Can I create and delete test records freely?
3. **Side effects:** Which actions have real-world consequences (emails, payments)?
4. **Known issues:** Are there historical bugs I should verify?
5. **Permissions:** Are there roles or accounts with different access?

### Default assumptions (when user provides no answers)

- Assume the main CRUD path is the core journey
- Assume creating test data is acceptable; deleting is not
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

## Scenarios

### 1. Main path (priority: high)
- [ ] Happy path: create → verify → edit → verify
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
If the user confirms or does not respond within one turn, proceed.

---

## Phase 4: Execution

**Goal:** Execute scenarios, record findings, maintain coverage.

### Execution loop

```
For each scenario (in priority order):
  1. Announce: "Executing: [scenario name]"
  2. Perform the steps using tabstride commands
  3. After each step: check document_changed, observe result
  4. If unexpected behavior → record a Finding
  5. Update Coverage Ledger
  6. If blocked → mark as blocked, move to next scenario
```

### Coverage Ledger (maintain throughout)

```markdown
# Coverage Ledger

## Covered
- [x] [scenario] — [result: pass/finding]

## Findings
- [F-1] [title] — [severity]

## Not covered
- [ ] [scenario] — [reason: blocked/out-of-scope/deferred]

## Blocked
- [ ] [scenario] — [what is needed to unblock]
```

Update the ledger after every scenario. The user can ask "progress?" at any
time to see the current ledger.

### Finding structure

When you discover unexpected behavior:

```markdown
### F-[N]: [Short title]

- **Severity:** critical / high / medium / low / cosmetic
- **Steps to reproduce:**
  1. [action]
  2. [action]
- **Expected:** [what should happen]
- **Actual:** [what did happen]
- **URL:** [current page]
- **Evidence:** [snapshot/screenshot/console output]
- **Reproducible:** yes / intermittent / once
```

### Evidence collection

For every finding, capture at minimum:

- The assertion or observation that revealed the issue
- `tabstride snapshot` of the current state
- Console errors if present (from error data or evidence)
- The exact sequence of commands that triggered it

Use `--json` on failing commands to capture structured evidence.

### Blocking rules

Mark a scenario as blocked when:

- It requires a permission or account you do not have
- It has irreversible side effects the user did not authorize
- The target feature is broken and cannot be reached
- You are uncertain whether an action is safe

Do not guess. Do not retry blocked scenarios without new information.

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

## Summary

[2-3 sentences: what was tested, key findings, overall impression]

## Feature Map

[The reconnaissance output from Phase 1]

## Coverage

| Category | Planned | Executed | Findings |
|----------|---------|----------|----------|
| Main path | N | N | N |
| Field boundaries | N | N | N |
| State/repetition | N | N | N |
| Error recovery | N | N | N |

## Findings

[All findings from Phase 4, ordered by severity]

## Risks and Boundaries

- [Areas that feel fragile but were not fully tested]
- [Edge cases that need dedicated test automation]

## Not Covered

- [What was not tested and why]

## Blocked

- [What could not be tested and what is needed]

## Recommendations

- [Suggested formal test cases to add]
- [Areas needing deeper exploration]
```

### Delivery

- Output the report as Markdown in the conversation
- If the user wants a file, write to a path they specify
- Always end with: "Session stopped. [N] findings, [M] scenarios covered."

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
| Output | Task result | Findings + report |
| Failure meaning | Task failed | Potential finding |

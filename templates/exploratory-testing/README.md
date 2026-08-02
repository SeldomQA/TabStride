# Exploratory Testing Template for TabStride

A methodology template that teaches AI agents how to perform systematic
exploratory testing using [TabStride](https://github.com/user/TabStride).

## What is this?

This is **not** a code library or a TabStride plugin. It is a structured
workflow guide (SKILL.md) that an AI agent loads to gain exploratory testing
expertise. It defines:

- How to safely observe a web application without modifying data
- What questions to ask the user before testing
- How to plan and prioritize test scenarios
- How to track coverage and record findings
- How to produce a final structured report

## Quick start

1. Install [TabStride](../../README.md) and ensure the extension is connected.
2. Copy or reference `SKILL.md` in your agent's skill directory.
3. Ask your agent: "Explore and test this page" while on the target website.

The agent will follow the five-phase lifecycle defined in SKILL.md:

```
Reconnaissance → Questions → Plan → Execution → Report
```

## File structure

```
templates/exploratory-testing/
├── SKILL.md                    ← Agent loads this (the methodology)
├── examples/
│   ├── feature-map.md          ← Phase 1 output example
│   ├── exploration-plan.md     ← Phase 3 output example
│   ├── coverage-ledger.md      ← Phase 4 tracking example
│   └── report.md               ← Phase 5 output example
└── README.md                   ← This file (for humans)
```

## When to use this vs. deterministic execution

| Scenario | Use |
|----------|-----|
| "Fill this form and submit" | Deterministic (Flow) |
| "Run these 10 steps and assert" | Deterministic (Flow) |
| "Test this page and find bugs" | **Exploratory (this template)** |
| "Explore the admin module" | **Exploratory (this template)** |
| "Smoke test the new release" | **Exploratory (this template)** |

## Requirements

- TabStride CLI installed and daemon running
- Browser extension connected
- A web page open in Chrome to test

## Customization

The SKILL.md is designed to be adapted:

- Adjust scope limits (max pages, navigation depth) for larger applications
- Add domain-specific question categories (e.g., compliance, accessibility)
- Extend the finding structure with fields your team needs (e.g., Jira ID)
- Modify the report format to match your organization's template

## Relationship to TabStride core

This template uses TabStride as infrastructure but does not modify it.
All capabilities it relies on (snapshot, persistent client, assertions,
evidence, screenshots) are part of the core TabStride release.

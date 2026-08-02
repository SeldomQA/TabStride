# Exploratory Testing Report

**Target:** TodoMVC Demo (https://demo.playwright.dev/todomvc/#/)
**Date:** 2026-08-01
**Duration:** ~15 minutes
**Session:** 9c41f7a2-3b8e-4d1a-a5f6-2e7c8d9b0a1f

## Summary

Explored the TodoMVC demo across 19 planned scenarios covering main CRUD
operations, field boundaries, state handling, and error recovery. Found 1
low-severity observation. The application is well-implemented with correct
filtering, trimming, and keyboard handling. No critical or high-severity
defects discovered.

## Feature Map

See [feature-map.md](feature-map.md) for the full reconnaissance output.

Key areas: task input, task list items (toggle/edit/delete), footer
(counter/filters/clear-completed).

## Coverage

| Category | Planned | Executed | Findings |
|----------|---------|----------|----------|
| Main path | 5 | 5 | 0 |
| Field boundaries | 6 | 6 | 1 |
| State/repetition | 5 | 5 | 0 |
| Error recovery | 4 | 3 | 0 |
| **Total** | **20** | **19** | **1** |

## Findings

### F-1: Script tag rendered as visible text in todo label

- **Severity:** low
- **Steps to reproduce:**
  1. Enter `<script>alert(1)</script>` in the todo input
  2. Press Enter
  3. Observe the created todo item
- **Expected:** Text displayed as literal string (escaped)
- **Actual:** Text displayed correctly as literal string; no script execution.
  However, raw HTML entities visible in DOM inspection without explicit
  escaping indicator — cosmetic concern only.
- **URL:** https://demo.playwright.dev/todomvc/#/
- **Evidence:** Snapshot shows `.todo-list li label` containing the text;
  no console errors; no script execution observed.
- **Reproducible:** yes

## Risks and Boundaries

- The demo uses localStorage; data persists across page reloads but not
  across browser profiles. No server-side validation exists.
- Inline edit mode (double-click) has no visible character limit — extremely
  long edits may cause layout overflow (not tested beyond 200 chars).
- "Clear completed" has no confirmation dialog — accidental data loss
  possible in a real application (acceptable for a demo).

## Not Covered

- Edit item to empty text (unclear expected behavior; deferred)
- Multiple browser tabs with shared localStorage

## Blocked

- None

## Recommendations

- Add a formal test case for XSS payloads in todo text (regression guard)
- Add a formal test case for edit-to-empty behavior (clarify spec)
- Consider testing with 100+ items for scroll performance (separate
  performance test, not exploratory)

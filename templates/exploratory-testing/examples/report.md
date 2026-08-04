# Exploratory Testing Report

**Target:** TodoMVC Demo (https://demo.playwright.dev/todomvc/#/)
**Date:** 2026-08-01
**Duration:** ~15 minutes
**Session:** 9c41f7a2-3b8e-4d1a-a5f6-2e7c8d9b0a1f
**Environment/build:** Public Playwright demo; build identifier not exposed
**Account role:** Anonymous visitor
**Browser/viewport:** Chrome desktop, 1440 × 900
**Start URL:** https://demo.playwright.dev/todomvc/#/
**Authorization:** Creating, editing, completing, and deleting demo todos

## Summary

Executed 18 of 20 planned TodoMVC scenarios covering main operations, field
boundaries, state handling, and error recovery. No confirmed defects were
found. Script-like input was safely rendered as literal text; two scenarios
remain untested because their expected behavior or feasible path was unclear.

## Feature Map

See [feature-map.md](feature-map.md) for the full reconnaissance output.

Key areas: task input, task list items (toggle/edit/delete), footer
(counter/filters/clear-completed).

## Coverage

| Category | Planned | Executed | Confirmed defects |
|----------|---------|----------|-------------------|
| Main path | 5 | 5 | 0 |
| Field boundaries | 6 | 5 | 0 |
| State/repetition | 5 | 5 | 0 |
| Error recovery | 4 | 3 | 0 |
| **Total** | **20** | **18** | **0** |

## Confirmed Defects

No confirmed defects.

## Risks, Questions, and Observations

- **O-1 — Script-like input handled safely:** `<script>alert(1)</script>` was
  displayed as literal text and did not execute. Oracle: browser security
  standard. Result: pass.
- **Q-1 — Edit to empty:** Expected behavior is not stated by the UI or an
  available requirement, so this was not classified as a defect.

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

- Delete item while in edit mode — no safe, reachable path was identified.

## Test Data and Cleanup

- **Cleaned:** All todos created by this exploration were removed.
- **Retained:** None.
- **Cleanup blocked:** None.

## Evidence Index

- **O-1:** Scenario 2.3 Snapshot and screenshot; no new Console errors.
- **Coverage:** Final Coverage Ledger and Test Data Ledger.

## Recommendations

- Preserve the passing script-like-input case as a security regression guard
- Add a formal test case for edit-to-empty behavior (clarify spec)
- Consider testing with 100+ items for scroll performance (separate
  performance test, not exploratory)

Session stopped. 0 confirmed defects, 18 scenarios covered.

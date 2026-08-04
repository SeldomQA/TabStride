# Coverage Ledger

**Last updated:** After scenario 3.2 (State and repetition)

## Covered

- [x] 1.1 Add three todos — **pass**
- [x] 1.2 Complete one todo — **pass**
- [x] 1.3 Edit todo text — **pass**
- [x] 1.4 Delete a todo — **pass**
- [x] 1.5 Filter All/Active/Completed — **pass**
- [x] 2.1 Empty input — **pass** (no item created)
- [x] 2.2 Long text (200 chars) — **pass** (displays correctly)
- [x] 2.3 Special characters — **pass** (rendered as literal text; no execution)
- [x] 2.4 Unicode input — **pass**
- [x] 2.5 Leading/trailing spaces — **pass** (trimmed)
- [x] 3.1 Rapid toggle — **pass** (no race condition)
- [x] 3.2 Duplicate text — **pass** (duplicates allowed)

## Confirmed defects

- None

## Risks / Questions / Observations

- **O-1:** Script-like input is rendered as literal text and does not execute;
  the security boundary behaved as expected.
- **Q-1:** Expected behavior for editing an item to an empty value is not yet
  established.

## Not covered

- [ ] 2.6 Edit item to empty text — deferred (need to verify expected behavior)
- [ ] 3.3 Complete all → Clear completed — next
- [ ] 3.4 Rapid "Clear completed" click
- [ ] 3.5 Refresh persistence
- [ ] 4.1–4.4 Error recovery scenarios

## Blocked

- [ ] None currently

## Test Data Ledger

| ID / identifying value | Scenario | Change | Cleanup authorized? | Final state |
|------------------------|----------|--------|---------------------|-------------|
| todo: EXP-meal | 1.1 | created | yes | retained until remaining scenarios finish |
| todo: EXP-code | 1.1 | created, edited | yes | retained until remaining scenarios finish |

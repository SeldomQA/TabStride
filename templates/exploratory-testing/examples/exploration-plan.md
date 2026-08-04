# Exploration Plan: TodoMVC Demo

**Objective:** Verify core todo operations and discover edge-case defects
**Scope:** Single page — input, list items, footer controls
**Data policy:** Free to create and delete todos (local storage, no server)
**Authorization:** Creating, editing, completing, and deleting demo todos authorized
**Cleanup policy:** Remove created todos and restore an empty list after testing
**Timebox:** 20 minutes, maximum 20 scenarios
**Stop conditions:** User cancellation, timebox exhausted, unsafe side effect, or no new evidence

## Scenarios

### 1. Main path (priority: high)

- [ ] Add three todos and verify all appear in list — Oracle: product contract
- [ ] Complete one todo and verify strikethrough + counter update — Oracle: consistency
- [ ] Edit a todo's text and verify persistence — Oracle: consistency after refresh
- [ ] Delete a todo and verify removal + counter update — Oracle: consistency
- [ ] Use All/Active/Completed filters and verify correct items shown — Oracle: link labels

### 2. Field boundaries (priority: high)

- [ ] Submit empty input (whitespace only) — Oracle: heuristic; do not call a defect without confirmation
- [ ] Submit very long text (200+ characters) — Oracle: layout remains usable
- [ ] Submit text with special characters: `<script>alert(1)</script>` — Oracle: browser security standard
- [ ] Submit text with Unicode: `日本語テスト 🎉` — Oracle: value consistency
- [ ] Submit text with leading/trailing spaces — Oracle: heuristic; clarify before calling a defect
- [ ] Edit item to empty text — Oracle: unknown; record a Question until clarified

### 3. State and repetition (priority: medium)

- [ ] Double-click toggle rapidly — Oracle: final UI and count remain consistent
- [ ] Add same text twice — Oracle: heuristic; observe whether duplicates are supported
- [ ] Complete all items → verify "Clear completed" appears — Oracle: visible UI contract
- [ ] Click "Clear completed" twice rapidly — Oracle: no error and consistent empty state
- [ ] Refresh page after adding items — Oracle: localStorage persistence contract

### 4. Error recovery (priority: medium)

- [ ] Edit item, then press Escape — Oracle: keyboard interaction contract
- [ ] Edit item, click elsewhere — Oracle: observed product behavior, then consistency
- [ ] Delete item while in edit mode (if possible) — Oracle: unknown; exploratory risk
- [ ] Check console for new errors after operations — Oracle: clean baseline comparison

### 5. Out of scope

- Mobile/touch interactions — desktop browser only
- Performance under 1000+ items — not a functional test
- Multiple browser tabs syncing — localStorage isolation assumed

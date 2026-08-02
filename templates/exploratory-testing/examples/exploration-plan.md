# Exploration Plan: TodoMVC Demo

**Objective:** Verify core todo operations and discover edge-case defects
**Scope:** Single page — input, list items, footer controls
**Data policy:** Free to create and delete todos (local storage, no server)

## Scenarios

### 1. Main path (priority: high)

- [ ] Add three todos and verify all appear in list
- [ ] Complete one todo and verify strikethrough + counter update
- [ ] Edit a todo's text and verify persistence
- [ ] Delete a todo and verify removal + counter update
- [ ] Use All/Active/Completed filters and verify correct items shown

### 2. Field boundaries (priority: high)

- [ ] Submit empty input (whitespace only) — should not create item
- [ ] Submit very long text (200+ characters)
- [ ] Submit text with special characters: `<script>alert(1)</script>`
- [ ] Submit text with Unicode: `日本語テスト 🎉`
- [ ] Submit text with leading/trailing spaces — should trim
- [ ] Edit item to empty text — should delete or reject

### 3. State and repetition (priority: medium)

- [ ] Double-click toggle rapidly (race condition)
- [ ] Add same text twice — should allow duplicates
- [ ] Complete all items → verify "Clear completed" appears
- [ ] Click "Clear completed" twice rapidly
- [ ] Refresh page after adding items — verify persistence (localStorage)

### 4. Error recovery (priority: medium)

- [ ] Edit item, then press Escape — should cancel edit
- [ ] Edit item, click elsewhere — should save edit
- [ ] Delete item while in edit mode (if possible)
- [ ] Check console for errors after all operations

### 5. Out of scope

- Mobile/touch interactions — desktop browser only
- Performance under 1000+ items — not a functional test
- Multiple browser tabs syncing — localStorage isolation assumed

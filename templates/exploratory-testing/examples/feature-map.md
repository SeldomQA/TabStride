# Feature Map: TodoMVC Demo

## Environment Baseline

- **Environment/build:** Public Playwright demo; build identifier not exposed
- **Account role:** Anonymous visitor
- **Browser/viewport:** Chrome desktop, 1440 × 900
- **Start URL:** https://demo.playwright.dev/todomvc/#/
- **Baseline:** Empty todo list; no pre-existing Console errors observed

## Task Input

- **URL:** https://demo.playwright.dev/todomvc/#/
- **Navigation:** Direct URL access
- **Forms:**
  - `What needs to be done?` — text input, creates a todo on Enter
- **Actions:**
  - Toggle checkbox — marks todo as completed/active
  - Double-click label — enters inline edit mode
  - "Clear completed" button — removes all completed todos
  - Filter links: All / Active / Completed
- **States:**
  - Empty (no todos): footer hidden
  - Populated: footer shows item count + filters
  - All completed: "Clear completed" visible
- **Side effects:** None (local storage only, no server)
- **Observations:** No console errors; clean single-page app

## Task List Item

- **URL:** Same page, list element
- **Navigation:** Add a todo first
- **Forms:**
  - Inline edit input (appears on double-click)
- **Actions:**
  - Toggle — complete/uncomplete
  - Double-click — edit text
  - Hover → destroy button (×) — delete item
- **States:**
  - Active (default)
  - Completed (strikethrough + `.completed` class)
  - Editing (input replaces label)
- **Side effects:** Delete is irreversible within session
- **Observations:** Destroy button only visible on hover

## Footer

- **URL:** Same page, footer section
- **Navigation:** Requires at least one todo
- **Actions:**
  - "All" / "Active" / "Completed" — filter visible list
  - "Clear completed" — bulk delete completed items
  - Item counter — displays remaining active count
- **States:**
  - Hidden when no todos exist
  - "Clear completed" hidden when nothing is completed
- **Side effects:** "Clear completed" permanently removes items
- **Observations:** Counter text uses singular/plural ("item" vs "items")

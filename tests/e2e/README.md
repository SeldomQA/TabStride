# Real Chrome E2E

The Actionability suite exercises the complete production path:

```text
tabstride CLI → serve/IPC → WebSocket → built MV3 extension → chrome.debugger/CDP → real page
```

It launches a disposable Chrome profile, loads the production extension build,
attaches the active fixture tab, and cleans up the session, service, profile,
and browser process when it finishes. It does not use mocked Chrome APIs.

Run the functional suite:

```bash
pnpm e2e:chrome
```

Run the fixed hot-path benchmark and compare its P95 with the checked-in
baseline. A regression greater than 15% fails:

```bash
pnpm perf:chrome
```

Run both:

```bash
pnpm test:chrome
```

Set `TABSTRIDE_CHROME=/path/to/chrome` when Chrome is not available in a
standard location. Pass `--headed` directly to
`scripts/actionability-chrome-e2e.mjs` for local diagnosis.

The scenario page covers delayed insertion, disabled-to-enabled transitions,
continuous movement, temporary obstruction, DOM replacement, strict
multi-match errors, structured timeout evidence, the in-page user Stop
control, attach-tab scope isolation, and CLI/Flow parity.

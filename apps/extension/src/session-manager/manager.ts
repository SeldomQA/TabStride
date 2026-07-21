import { AGENT_WINDOW_HOME, type AgentWindowApi, chromeAgentWindowApi } from "./agent-window";
import { RefStore } from "./ref-store";

export interface SessionContext {
  sessionId: string;
  mode: "isolated" | "attach";
  /** Dedicated Agent Window for isolated sessions; the user's existing
   * window for attach sessions. Never close this id when mode=attach. */
  agentWindowId: number;
  /** The only leased tab for attach sessions. */
  attachedTabId?: number;
  refStore: RefStore;
  borrowedTabs: Map<number, BorrowedTab>;
  createdAtMs: number;
}

export interface BorrowedTab {
  tabId: number;
  originalWindowId: number;
  originalIndex: number;
}

export interface BorrowReservation {
  release(): void;
  commit(entry: BorrowedTab): void;
}

export interface SessionManagerOptions {
  agentWindow?: AgentWindowApi;
  now?: () => number;
}

/**
 * Owner of all live agent sessions inside the extension.
 *
 * The daemon side has its own `SessionRegistry`; this class is the
 * extension-side mirror that holds the per-session Agent Window id,
 * ref-store, and borrowed-tab table. Tool implementations (M6+) read
 * from here to map a `session_id` back to "which Chrome window /
 * which ref / which borrowed tab".
 *
 * Designed to be unit-testable: chrome.* is injected via `AgentWindowApi`
 * so vitest never touches a real `chrome.windows` object.
 */
export class SessionManager {
  private readonly sessions = new Map<string, SessionContext>();
  private readonly windowIndex = new Map<number, string>();
  private readonly attachedTabIndex = new Map<number, string>();
  private readonly borrowReservations = new Map<number, string>();
  private readonly agentWindow: AgentWindowApi;
  private readonly now: () => number;

  constructor(options: SessionManagerOptions = {}) {
    this.agentWindow = options.agentWindow ?? chromeAgentWindowApi;
    this.now = options.now ?? Date.now;
  }

  has(sessionId: string): boolean {
    return this.sessions.has(sessionId);
  }

  get(sessionId: string): SessionContext | null {
    return this.sessions.get(sessionId) ?? null;
  }

  findByWindowId(windowId: number): SessionContext | null {
    const id = this.windowIndex.get(windowId);
    return id ? (this.sessions.get(id) ?? null) : null;
  }

  findByAttachedTabId(tabId: number): SessionContext | null {
    const id = this.attachedTabIndex.get(tabId);
    return id ? (this.sessions.get(id) ?? null) : null;
  }

  findByTabId(tabId: number): SessionContext | null {
    const attached = this.findByAttachedTabId(tabId);
    if (attached) return attached;
    for (const ctx of this.sessions.values()) {
      if (ctx.borrowedTabs.has(tabId)) return ctx;
    }
    return null;
  }

  list(): SessionContext[] {
    return Array.from(this.sessions.values());
  }

  /**
   * Look up whether `tabId` is currently borrowed by some *other*
   * session than the one calling. Used by M8 `tab_borrow` to refuse
   * a second borrow on the same Chrome tab, and by `tab_close` to
   * tell apart "user tab" from "another session's borrowed tab"
   * (which we must not allow direct access to).
   *
   * Returns the borrowing session id when applicable, otherwise null.
   */
  findBorrowingSession(tabId: number, currentSessionId: string | null): string | null {
    const attachedBy = this.attachedTabIndex.get(tabId);
    if (attachedBy && attachedBy !== currentSessionId) return attachedBy;
    for (const ctx of this.sessions.values()) {
      if (ctx.sessionId === currentSessionId) continue;
      if (ctx.borrowedTabs.has(tabId)) return ctx.sessionId;
    }
    const reservedBy = this.borrowReservations.get(tabId);
    if (reservedBy && reservedBy !== currentSessionId) return reservedBy;
    return null;
  }

  /**
   * Reserve a tab for `tool.tab_borrow` before the handler performs any
   * awaited Chrome work. This closes the cross-session race between the
   * "is anyone borrowing this tab?" check and the eventual borrowedTabs
   * write after `chrome.tabs.move`.
   */
  tryReserveBorrow(tabId: number, sessionId: string): BorrowReservation | { borrowedBy: string } {
    const borrowedBy = this.findBorrowingSession(tabId, sessionId);
    if (borrowedBy) return { borrowedBy };
    this.borrowReservations.set(tabId, sessionId);
    let closed = false;
    const release = () => {
      if (closed) return;
      closed = true;
      if (this.borrowReservations.get(tabId) === sessionId) {
        this.borrowReservations.delete(tabId);
      }
    };
    return {
      release,
      commit: (entry) => {
        if (closed) return;
        const ctx = this.sessions.get(sessionId);
        if (!ctx) {
          release();
          throw new Error(`session ${sessionId} disappeared during tab_borrow`);
        }
        if (this.borrowReservations.get(tabId) !== sessionId) {
          throw new Error(`tab ${tabId} borrow reservation disappeared before commit`);
        }
        ctx.borrowedTabs.set(tabId, entry);
        release();
      },
    };
  }

  /**
   * Spin up a fresh session: open a new Agent Window with an
   * `about:blank` tab and register the context.
   *
   * Returns the created window id so callers can echo it back to the
   * daemon in the `tool.session_start` reply.
   */
  async start(sessionId: string): Promise<SessionContext> {
    if (this.sessions.has(sessionId)) {
      throw new Error(`[bh] session ${sessionId} already exists`);
    }
    const windowId = await this.agentWindow.create(AGENT_WINDOW_HOME);
    await this.agentWindow.ensureActiveTab(windowId, AGENT_WINDOW_HOME);
    const ctx: SessionContext = {
      sessionId,
      mode: "isolated",
      agentWindowId: windowId,
      refStore: new RefStore(),
      borrowedTabs: new Map(),
      createdAtMs: this.now(),
    };
    this.sessions.set(sessionId, ctx);
    this.windowIndex.set(windowId, sessionId);
    return ctx;
  }

  /** Register an in-place lease on one existing user tab. No window or tab
   * is created, moved, focused, or closed by this operation. */
  startAttached(sessionId: string, tabId: number, windowId: number): SessionContext {
    if (this.sessions.has(sessionId)) {
      throw new Error(`[bh] session ${sessionId} already exists`);
    }
    const leasedBy = this.attachedTabIndex.get(tabId);
    if (leasedBy) {
      throw new Error(`tab ${tabId} is already attached by session ${leasedBy}`);
    }
    const borrowedBy = this.findBorrowingSession(tabId, sessionId);
    if (borrowedBy) {
      throw new Error(`tab ${tabId} is already controlled by session ${borrowedBy}`);
    }
    if (this.findByWindowId(windowId)) {
      throw new Error(`tab ${tabId} belongs to an Agent Window`);
    }
    const ctx: SessionContext = {
      sessionId,
      mode: "attach",
      agentWindowId: windowId,
      attachedTabId: tabId,
      refStore: new RefStore(),
      borrowedTabs: new Map(),
      createdAtMs: this.now(),
    };
    this.sessions.set(sessionId, ctx);
    this.attachedTabIndex.set(tabId, sessionId);
    return ctx;
  }

  /**
   * Tear down a session: close its Agent Window and drop the context.
   *
   * `dropOnly = true` skips closing the window — used when the user
   * already closed it manually (M5.4 path) so we don't accidentally
   * close a window that has been re-purposed.
   */
  async stop(
    sessionId: string,
    options: { dropOnly?: boolean } = {},
  ): Promise<SessionContext | null> {
    const ctx = this.sessions.get(sessionId);
    if (!ctx) return null;
    if (!options.dropOnly && ctx.mode === "isolated") {
      await this.agentWindow.remove(ctx.agentWindowId);
    }
    this.sessions.delete(sessionId);
    if (ctx.mode === "isolated") {
      this.windowIndex.delete(ctx.agentWindowId);
    } else if (ctx.attachedTabId !== undefined) {
      this.attachedTabIndex.delete(ctx.attachedTabId);
    }
    return ctx;
  }

  /**
   * Best-effort cleanup of every live session (emergency brake / SW
   * shutdown). Returns the set of `session_id`s that were removed.
   */
  async stopAll(options: { dropOnly?: boolean } = {}): Promise<string[]> {
    const ids = Array.from(this.sessions.keys());
    for (const id of ids) {
      await this.stop(id, options);
    }
    return ids;
  }
}

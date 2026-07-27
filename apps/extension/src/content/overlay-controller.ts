import type { OverlayOperationLogEntry } from "@/lib/overlay-bridge";
import type { BorrowRequestData } from "./BorrowConfirmationOverlay";
import type { HelpRequestData } from "./HelpRequestOverlay";

export interface OverlayState {
  borrowRequests: BorrowRequestData[];
  activeHelp: HelpRequestData | null;
  controlVisible: boolean;
  interrupting: boolean;
  activeSessionId: string | null;
  automationBypassCount: number;
  operationLogs: OverlayOperationLogEntry[];
}

type MutableOverlayState = Omit<OverlayState, "controlVisible">;

/**
 * Owns overlay state by rendering scope. User-tab overlays survive Agent
 * session teardown; Agent-tab overlays are cleared whenever a tab is returned.
 */
export class OverlayController {
  private state: MutableOverlayState = {
    borrowRequests: [],
    activeHelp: null,
    interrupting: false,
    activeSessionId: null,
    automationBypassCount: 0,
    operationLogs: [],
  };

  snapshot(): OverlayState {
    return {
      ...this.state,
      controlVisible: this.isControlVisible(),
      borrowRequests: [...this.state.borrowRequests],
      operationLogs: this.state.operationLogs.map((entry) => ({ ...entry })),
    };
  }

  isControlVisible(): boolean {
    return this.state.activeSessionId !== null;
  }

  addBorrowRequest(request: BorrowRequestData): void {
    this.state.borrowRequests = [
      ...this.state.borrowRequests.filter((r) => r.id !== request.id),
      request,
    ];
  }

  removeBorrowRequest(requestId: string): void {
    this.state.borrowRequests = this.state.borrowRequests.filter((r) => r.id !== requestId);
  }

  activateAgentSession(sessionId: string): void {
    if (this.state.activeSessionId !== sessionId) {
      this.state.operationLogs = [];
    }
    this.state.activeSessionId = sessionId;
    this.state.interrupting = false;
  }

  setInterrupting(interrupting: boolean): void {
    if (!this.state.activeSessionId) return;
    this.state.interrupting = interrupting;
  }

  setAgentHelpRequest(request: HelpRequestData): HelpRequestData | null {
    const previous = this.state.activeHelp;
    this.state.activeHelp = request;
    return previous;
  }

  clearAgentHelpRequest(requestId?: string): void {
    if (!this.state.activeHelp) return;
    if (requestId && this.state.activeHelp.id !== requestId) return;
    this.state.activeHelp = null;
  }

  setAutomationBypass(enabled: boolean): void {
    if (enabled) {
      this.state.automationBypassCount += 1;
    } else {
      this.state.automationBypassCount = Math.max(0, this.state.automationBypassCount - 1);
    }
  }

  setOperationLogs(sessionId: string, logs: OverlayOperationLogEntry[]): void {
    if (this.state.activeSessionId !== sessionId) return;
    this.state.operationLogs = logs.map((entry) => ({ ...entry }));
  }

  resetAgentOverlays(sessionId: string): HelpRequestData | null {
    if (this.state.activeSessionId && this.state.activeSessionId !== sessionId) {
      return null;
    }
    const previousHelp = this.state.activeHelp;
    this.state = {
      ...this.state,
      activeHelp: null,
      interrupting: false,
      activeSessionId: null,
      automationBypassCount: 0,
      operationLogs: [],
    };
    return previousHelp;
  }
}

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export interface SessionInfo {
  id: string;
  name: string;
  projectPath: string;
  cliType: string;
  status: string;
  createdAt: string;
  lastActiveAt: string;
}

export interface Activity {
  id: string;
  type: string;
  content: string;
  timestamp: string;
  [key: string]: unknown;
}

export type ClientMessage =
  | { type: 'get_sessions' }
  | { type: 'subscribe'; session_id: string }
  | { type: 'unsubscribe'; session_id: string }
  | { type: 'send_input'; session_id: string; text: string }
  | { type: 'tool_approval'; session_id: string; approval_id: string; approved: boolean; always: boolean }
  | { type: 'ping' };

export type ServerMessage =
  | { type: 'sessions_list'; sessions: SessionInfo[] }
  | { type: 'activity_update'; session_id: string; activity: Activity }
  | { type: 'session_status'; session_id: string; status: string }
  | { type: 'tool_approval_request'; session_id: string; approval_id: string; tool_name: string; params: Record<string, unknown> }
  | { type: 'error'; message: string }
  | { type: 'pong' };

interface ToolApprovalRequest {
  sessionId: string;
  approvalId: string;
  toolName: string;
  params: Record<string, unknown>;
}

interface ClientSyncState {
  sessions: SessionInfo[];
  activities: Map<string, Activity[]>;
  connected: boolean;
  connecting: boolean;
  error: string | null;
  pendingApprovals: ToolApprovalRequest[];

  // Actions
  connect: (relayUrl: string, roomCode: string, key: string) => Promise<void>;
  disconnect: () => Promise<void>;
  requestSessions: () => Promise<void>;
  subscribeToSession: (sessionId: string) => Promise<void>;
  sendInput: (sessionId: string, text: string) => Promise<void>;
  sendToolApproval: (sessionId: string, approvalId: string, approved: boolean, always?: boolean) => Promise<void>;
  clearError: () => void;

  // Internal - for event handlers
  _handleMessage: (msg: ServerMessage) => void;
  _setConnected: (connected: boolean) => void;
}

let unlistenMessage: UnlistenFn | null = null;
let unlistenStatus: UnlistenFn | null = null;

export const useClientSyncStore = create<ClientSyncState>((set, get) => ({
  sessions: [],
  activities: new Map(),
  connected: false,
  connecting: false,
  error: null,
  pendingApprovals: [],

  connect: async (relayUrl: string, roomCode: string, key: string) => {
    set({ connecting: true, error: null });

    try {
      // Set up event listeners before connecting
      unlistenMessage = await listen<ServerMessage>('client-message', (event) => {
        get()._handleMessage(event.payload);
      });

      unlistenStatus = await listen<string>('client-status', (event) => {
        const connected = event.payload === 'connected';
        get()._setConnected(connected);
        if (!connected) {
          set({ connecting: false });
        }
      });

      await invoke('connect_as_client', { relayUrl, roomCode, key });

      // Request sessions immediately after connecting
      await get().requestSessions();

    } catch (e) {
      // Clean up listeners on error
      if (unlistenMessage) {
        unlistenMessage();
        unlistenMessage = null;
      }
      if (unlistenStatus) {
        unlistenStatus();
        unlistenStatus = null;
      }
      set({ error: String(e), connecting: false, connected: false });
      throw e;
    }
  },

  disconnect: async () => {
    try {
      await invoke('disconnect_client');
    } finally {
      // Clean up listeners
      if (unlistenMessage) {
        unlistenMessage();
        unlistenMessage = null;
      }
      if (unlistenStatus) {
        unlistenStatus();
        unlistenStatus = null;
      }
      set({
        connected: false,
        connecting: false,
        sessions: [],
        activities: new Map(),
        pendingApprovals: [],
      });
    }
  },

  requestSessions: async () => {
    try {
      await invoke('request_sessions_from_host');
    } catch (e) {
      set({ error: String(e) });
    }
  },

  subscribeToSession: async (sessionId: string) => {
    try {
      await invoke('subscribe_to_session', { sessionId });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  sendInput: async (sessionId: string, text: string) => {
    try {
      await invoke('send_input_to_host', { sessionId, text });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  sendToolApproval: async (sessionId: string, approvalId: string, approved: boolean, always = false) => {
    try {
      await invoke('send_tool_approval_to_host', {
        sessionId,
        approvalId,
        approved,
        always,
      });
      // Remove from pending approvals
      set((state) => ({
        pendingApprovals: state.pendingApprovals.filter((a) => a.approvalId !== approvalId),
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },

  clearError: () => set({ error: null }),

  _handleMessage: (msg: ServerMessage) => {
    switch (msg.type) {
      case 'sessions_list':
        set({ sessions: msg.sessions });
        break;

      case 'activity_update':
        set((state) => {
          const newActivities = new Map(state.activities);
          const existing = newActivities.get(msg.session_id) || [];
          newActivities.set(msg.session_id, [...existing, msg.activity]);
          return { activities: newActivities };
        });
        break;

      case 'session_status':
        set((state) => ({
          sessions: state.sessions.map((s) =>
            s.id === msg.session_id ? { ...s, status: msg.status } : s
          ),
        }));
        break;

      case 'tool_approval_request':
        set((state) => ({
          pendingApprovals: [
            ...state.pendingApprovals,
            {
              sessionId: msg.session_id,
              approvalId: msg.approval_id,
              toolName: msg.tool_name,
              params: msg.params,
            },
          ],
        }));
        break;

      case 'error':
        set({ error: msg.message });
        break;

      case 'pong':
        // Keepalive response - no action needed
        break;
    }
  },

  _setConnected: (connected: boolean) => {
    set({ connected, connecting: false });
  },
}));

// Convenience hook
export function useClientSync() {
  const store = useClientSyncStore();
  return {
    sessions: store.sessions,
    activities: store.activities,
    connected: store.connected,
    connecting: store.connecting,
    error: store.error,
    pendingApprovals: store.pendingApprovals,
    connect: store.connect,
    disconnect: store.disconnect,
    requestSessions: store.requestSessions,
    subscribeToSession: store.subscribeToSession,
    sendInput: store.sendInput,
    sendToolApproval: store.sendToolApproval,
    clearError: store.clearError,
  };
}

import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface Session {
  id: string;
  name: string;
  projectPath: string;
  createdAt: string;
  lastActiveAt: string;
  status: 'active' | 'idle' | 'closed';
  conversationId?: string | null;
  cliType: 'claude' | 'gemini' | 'codex' | 'opencode';
}

export interface CliInfo {
  id: string;
  name: string;
  installed: boolean;
  supportsResume: boolean;
}

export interface Message {
  id: string;
  sessionId: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  toolName?: string;
  toolResult?: string;
  timestamp: string;
  isStreaming?: boolean;
}

export interface WaitingState {
  sessionId: string;
  waitType: 'tool_approval' | 'plan_approval' | 'clarifying_question' | 'awaiting_response' | null;
  promptContent?: string;
  timestamp: string;
}

// ISSUE #5: Track input state for "User typing" indicator
export interface InputState {
  text: string;
  cursorPosition: number;
  timestamp: number;
}

interface SessionState {
  sessions: Session[];
  activeSessionId: string | null;
  messages: Record<string, Message[]>; // sessionId -> messages
  waitingStates: Record<string, WaitingState>; // sessionId -> waiting state
  inputStates: Record<string, InputState>; // ISSUE #5: sessionId -> input state for typing indicator
  availableClis: CliInfo[];
  isLoading: boolean;
  error: string | null;

  // Actions
  fetchSessions: () => Promise<void>;
  fetchAvailableClis: () => Promise<void>;
  setActiveSession: (sessionId: string | null) => void;
  createSession: (projectPath: string, name?: string, cliType?: string) => Promise<Session>;
  closeSession: (sessionId: string) => Promise<void>;
  renameSession: (sessionId: string, newName: string) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;
  resumeSession: (sessionId: string) => Promise<Session>;
  fetchMessages: (sessionId: string) => Promise<void>;
  addMessage: (sessionId: string, message: Message) => void;
  updateMessage: (messageId: string, content: string) => void;
  sendInput: (sessionId: string, input: string) => Promise<void>;
  setWaitingState: (sessionId: string, state: WaitingState | null) => void;
  setInputState: (sessionId: string, state: InputState | null) => void; // ISSUE #5
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  messages: {},
  waitingStates: {},
  inputStates: {}, // ISSUE #5: Track input states for typing indicator
  availableClis: [],
  isLoading: false,
  error: null,

  fetchSessions: async () => {
    set({ isLoading: true, error: null });
    try {
      const sessions = await invoke<Session[]>('get_sessions');
      // Deduplicate sessions by ID (prevents race condition duplicates)
      const uniqueSessions = sessions.filter(
        (session, index, self) => self.findIndex(s => s.id === session.id) === index
      );
      set({ sessions: uniqueSessions, isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },

  fetchAvailableClis: async () => {
    try {
      const clis = await invoke<CliInfo[]>('get_available_clis');
      set({ availableClis: clis });
    } catch (e) {
      console.error('Failed to fetch available CLIs:', e);
    }
  },

  setActiveSession: (sessionId) => {
    set({ activeSessionId: sessionId });
    if (sessionId && !get().messages[sessionId]) {
      get().fetchMessages(sessionId);
    }
  },

  createSession: async (projectPath, name, cliType) => {
    set({ isLoading: true, error: null });
    try {
      const session = await invoke<Session>('create_session', {
        request: { project_path: projectPath, name, cli_type: cliType },
      });
      // FIX: Prevent duplicate sessions from race condition with session-created event
      // The Tauri command emits session-created which triggers fetchSessions()
      // This can race with our direct state update, causing duplicates
      set((state) => {
        const exists = state.sessions.some(s => s.id === session.id);
        if (exists) {
          // Session already added by fetchSessions() - just set it active
          return { activeSessionId: session.id, isLoading: false };
        }
        return {
          sessions: [session, ...state.sessions],
          activeSessionId: session.id,
          isLoading: false,
        };
      });
      return session;
    } catch (e) {
      set({ error: String(e), isLoading: false });
      throw e;
    }
  },

  closeSession: async (sessionId) => {
    try {
      await invoke('close_session', { sessionId });
      set((state) => ({
        sessions: state.sessions.map((s) =>
          s.id === sessionId ? { ...s, status: 'closed' as const } : s
        ),
        activeSessionId:
          state.activeSessionId === sessionId ? null : state.activeSessionId,
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },

  renameSession: async (sessionId, newName) => {
    const name = newName.trim();
    if (!name) {
      throw new Error('Session name cannot be empty');
    }

    try {
      await invoke('rename_session', { sessionId, newName: name });
      set((state) => ({
        sessions: state.sessions.map((s) =>
          s.id === sessionId ? { ...s, name } : s
        ),
      }));
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  deleteSession: async (sessionId) => {
    try {
      await invoke('delete_session', { sessionId });
      set((state) => ({
        sessions: state.sessions.filter((s) => s.id !== sessionId),
        activeSessionId:
          state.activeSessionId === sessionId ? null : state.activeSessionId,
        // Also remove messages for this session
        messages: Object.fromEntries(
          Object.entries(state.messages).filter(([id]) => id !== sessionId)
        ),
      }));
    } catch (e) {
      set({ error: String(e) });
      throw e; // Re-throw to let caller handle
    }
  },

  resumeSession: async (sessionId) => {
    set({ isLoading: true, error: null });
    try {
      const session = await invoke<Session>('resume_session', { sessionId });
      set((state) => ({
        sessions: state.sessions.map((s) =>
          s.id === sessionId ? session : s
        ),
        activeSessionId: session.id,
        isLoading: false,
      }));
      return session;
    } catch (e) {
      set({ error: String(e), isLoading: false });
      throw e;
    }
  },

  fetchMessages: async (sessionId) => {
    try {
      // Only fetch from DB if we don't have messages for this session
      const currentMessages = get().messages[sessionId];
      if (currentMessages && currentMessages.length > 0) {
        // Already have messages in memory, don't overwrite
        return;
      }

      const messages = await invoke<Message[]>('get_messages', {
        sessionId,
        limit: 100,
      });
      set((state) => ({
        messages: { ...state.messages, [sessionId]: messages },
      }));
    } catch (e) {
      set({ error: String(e) });
    }
  },

  addMessage: (sessionId, message) => {
    set((state) => ({
      messages: {
        ...state.messages,
        [sessionId]: [...(state.messages[sessionId] || []), message],
      },
    }));
  },

  updateMessage: (messageId, content) => {
    set((state) => {
      const newMessages = { ...state.messages };
      for (const sessionId in newMessages) {
        newMessages[sessionId] = newMessages[sessionId].map((m) =>
          m.id === messageId ? { ...m, content } : m
        );
      }
      return { messages: newMessages };
    });
  },

  sendInput: async (sessionId, input) => {
    // Optimistically add user message
    const userMessage: Message = {
      id: crypto.randomUUID(),
      sessionId,
      role: 'user',
      content: input,
      timestamp: new Date().toISOString(),
    };
    get().addMessage(sessionId, userMessage);

    try {
      await invoke('send_input', { sessionId, input });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setWaitingState: (sessionId, state) => {
    set((current) => {
      if (state === null) {
        // Remove the waiting state
        const { [sessionId]: _, ...rest } = current.waitingStates;
        return { waitingStates: rest };
      }
      return {
        waitingStates: { ...current.waitingStates, [sessionId]: state },
      };
    });
  },

  // ISSUE #5: Track input state for "User typing" indicator
  setInputState: (sessionId, state) => {
    set((current) => {
      if (state === null) {
        const { [sessionId]: _, ...rest } = current.inputStates;
        return { inputStates: rest };
      }
      return {
        inputStates: { ...current.inputStates, [sessionId]: state },
      };
    });
  },
}));

// MobileCLI Shared Types
// These types are used by both desktop (Tauri) and mobile (Expo) apps

export interface Session {
  id: string;
  name: string;
  projectPath: string;
  createdAt: string;
  lastActiveAt: string;
  status: SessionStatus;
}

export type SessionStatus = 'active' | 'idle' | 'closed';

export interface Message {
  id: string;
  sessionId: string;
  role: MessageRole;
  content: string;
  toolName?: string;
  toolResult?: string;
  timestamp: string;
  isStreaming?: boolean;
}

export type MessageRole = 'user' | 'assistant' | 'system' | 'tool';

// Tool call representation for collapsible UI
export interface ToolCall {
  id: string;
  messageId: string;
  name: string;
  input?: string;
  output?: string;
  status: 'pending' | 'running' | 'completed' | 'error';
  startedAt: string;
  completedAt?: string;
}

// Session state machine
export type SessionState =
  | 'idle'           // Waiting for user input
  | 'user_typing'    // User is typing (for UI feedback)
  | 'processing'     // Claude is thinking/processing
  | 'responding'     // Claude is streaming response
  | 'tool_running';  // A tool is being executed

// Connection state for mobile app
export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'error';

// App settings
export interface AppSettings {
  serverUrl: string;
  authToken?: string;
  theme: 'light' | 'dark' | 'system';
  notifications: boolean;
}

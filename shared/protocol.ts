// MobileCLI WebSocket Protocol
// Defines all message types for desktop <-> mobile communication

import type { Session, Message, SessionState, ToolCall } from './types';

// ============================================
// Client -> Server Messages (Mobile -> Desktop)
// ============================================

export interface ClientHello {
  type: 'hello';
  authToken?: string;
  clientVersion: string;
}

export interface ClientSubscribe {
  type: 'subscribe';
  sessionId: string;
}

export interface ClientUnsubscribe {
  type: 'unsubscribe';
  sessionId: string;
}

export interface ClientSendInput {
  type: 'send_input';
  sessionId: string;
  text: string;
}

export interface ClientCreateSession {
  type: 'create_session';
  projectPath: string;
  name?: string;
}

export interface ClientCloseSession {
  type: 'close_session';
  sessionId: string;
}

export interface ClientGetSessions {
  type: 'get_sessions';
}

export interface ClientGetMessages {
  type: 'get_messages';
  sessionId: string;
  limit?: number;
  before?: string; // cursor for pagination
}

export type ClientMessage =
  | ClientHello
  | ClientSubscribe
  | ClientUnsubscribe
  | ClientSendInput
  | ClientCreateSession
  | ClientCloseSession
  | ClientGetSessions
  | ClientGetMessages;

// ============================================
// Server -> Client Messages (Desktop -> Mobile)
// ============================================

export interface ServerWelcome {
  type: 'welcome';
  serverVersion: string;
  authenticated: boolean;
}

export interface ServerError {
  type: 'error';
  code: string;
  message: string;
  requestType?: string;
}

export interface ServerSessions {
  type: 'sessions';
  sessions: Session[];
}

export interface ServerSessionCreated {
  type: 'session_created';
  session: Session;
}

export interface ServerSessionUpdated {
  type: 'session_updated';
  sessionId: string;
  changes: Partial<Session>;
}

export interface ServerSessionClosed {
  type: 'session_closed';
  sessionId: string;
}

export interface ServerMessages {
  type: 'messages';
  sessionId: string;
  messages: Message[];
  hasMore: boolean;
}

export interface ServerNewMessage {
  type: 'new_message';
  message: Message;
}

export interface ServerMessageUpdate {
  type: 'message_update';
  messageId: string;
  content: string;        // Full content (for streaming updates)
  isComplete: boolean;
}

export interface ServerStateChange {
  type: 'state_change';
  sessionId: string;
  state: SessionState;
}

export interface ServerToolCall {
  type: 'tool_call';
  sessionId: string;
  toolCall: ToolCall;
}

export interface ServerToolUpdate {
  type: 'tool_update';
  toolCallId: string;
  status: ToolCall['status'];
  output?: string;
}

export type ServerMessage =
  | ServerWelcome
  | ServerError
  | ServerSessions
  | ServerSessionCreated
  | ServerSessionUpdated
  | ServerSessionClosed
  | ServerMessages
  | ServerNewMessage
  | ServerMessageUpdate
  | ServerStateChange
  | ServerToolCall
  | ServerToolUpdate;

// ============================================
// Utility Types
// ============================================

export type WSMessage = ClientMessage | ServerMessage;

// Type guards for message handling
export function isClientMessage(msg: WSMessage): msg is ClientMessage {
  return ['hello', 'subscribe', 'unsubscribe', 'send_input',
          'create_session', 'close_session', 'get_sessions',
          'get_messages'].includes(msg.type);
}

export function isServerMessage(msg: WSMessage): msg is ServerMessage {
  return ['welcome', 'error', 'sessions', 'session_created',
          'session_updated', 'session_closed', 'messages',
          'new_message', 'message_update', 'state_change',
          'tool_call', 'tool_update'].includes(msg.type);
}

// Protocol version for compatibility checking
export const PROTOCOL_VERSION = '1.0.0';

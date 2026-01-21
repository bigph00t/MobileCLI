import { useCallback, useEffect, useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Session, useSessionStore } from '../hooks/useSession';
import Terminal from './Terminal';
import { TypingIndicator } from './TypingIndicator';

interface ClaudeMessage {
  role: string;
  content: string;
  timestamp?: string;
}

interface ChatViewProps {
  session: Session;
  onClose: () => void;
}

export default function ChatView({ session, onClose }: ChatViewProps) {
  const { resumeSession } = useSessionStore();
  const [isResuming, setIsResuming] = useState(false);
  const [claudeHistory, setClaudeHistory] = useState<ClaudeMessage[]>([]);
  const [isLoadingHistory, setIsLoadingHistory] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Fetch Claude's conversation history for closed sessions
  useEffect(() => {
    if (session.status === 'closed' && session.conversationId) {
      setIsLoadingHistory(true);
      invoke<ClaudeMessage[]>('get_claude_history', {
        projectPath: session.projectPath,
        conversationId: session.conversationId,
        limit: 50,
      })
        .then((messages) => {
          setClaudeHistory(messages);
        })
        .catch((e) => {
          console.error('Failed to load Claude history:', e);
        })
        .finally(() => {
          setIsLoadingHistory(false);
        });
    }
  }, [session.id, session.status, session.conversationId, session.projectPath]);

  // Scroll to bottom when messages load
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [claudeHistory]);

  // Handle terminal input - send directly to PTY
  const handleTerminalData = useCallback(
    async (data: string) => {
      // Don't send input if session is not active
      if (session.status !== 'active') {
        return;
      }

      try {
        // Send raw input to PTY (including special keys)
        await invoke('send_raw_input', { sessionId: session.id, input: data });
      } catch (e) {
        console.error('[ChatView] Failed to send input:', e);
      }
    },
    [session.id, session.status]
  );

  // Close session handler - available for future UI button
  const _handleClose = async () => {
    if (confirm('Close this session?')) {
      try {
        await invoke('close_session', { sessionId: session.id });
        onClose();
      } catch (e) {
        console.error('Failed to close session:', e);
      }
    }
  };
  void _handleClose; // Suppress unused warning

  const handleResume = async () => {
    setIsResuming(true);
    try {
      await resumeSession(session.id);
    } catch (e) {
      console.error('Failed to resume session:', e);
      alert('Failed to resume session: ' + String(e));
    } finally {
      setIsResuming(false);
    }
  };

  // Extract project name from path
  const projectName = session.projectPath.split('/').pop() || session.projectPath;

  return (
    <div className="flex-1 flex flex-col h-full bg-[#1a1b26]">
      {/* Terminal-style Header */}
      <header className="flex items-center justify-between px-3 py-2 bg-[#16161e] border-b border-[#414868]/50">
        <div className="flex items-center gap-3">
          {/* Session info */}
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-[#c0caf5]">
              {projectName}
            </span>
            <span className="text-xs text-[#565f89]">—</span>
            <span className="text-xs text-[#7aa2f7]">{session.cliType}</span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {/* Status indicator */}
          <span
            className={`flex items-center gap-1.5 px-2 py-0.5 rounded text-xs font-medium ${
              session.status === 'active'
                ? 'text-[#9ece6a]'
                : session.status === 'idle'
                ? 'text-[#e0af68]'
                : 'text-[#565f89]'
            }`}
          >
            <span className={`w-1.5 h-1.5 rounded-full ${
              session.status === 'active'
                ? 'bg-[#9ece6a] animate-pulse'
                : session.status === 'idle'
                ? 'bg-[#e0af68]'
                : 'bg-[#565f89]'
            }`} />
            {session.status}
          </span>
        </div>
      </header>

      {/* Content area */}
      <div className="flex-1 overflow-hidden relative">
        {session.status === 'closed' ? (
          // Show message history for closed sessions
          <div className="h-full flex flex-col">
            {/* Message history */}
            <div className="flex-1 overflow-y-auto p-4 space-y-4 bg-[#1a1b26]">
              {isLoadingHistory ? (
                <div className="text-center text-[#565f89] py-8 font-mono">
                  Loading conversation history...
                </div>
              ) : claudeHistory.length === 0 ? (
                <div className="text-center text-[#565f89] py-8 font-mono">
                  {session.conversationId
                    ? 'No conversation history found'
                    : 'No conversation ID - cannot load history'}
                </div>
              ) : (
                <>
                  {claudeHistory.map((msg, idx) => (
                    <MessageBubble key={idx} message={msg} />
                  ))}
                  <div ref={messagesEndRef} />
                </>
              )}
            </div>

            {/* Resume overlay */}
            {session.conversationId && (
              <div className="absolute inset-0 bg-[#1a1b26]/80 backdrop-blur-sm flex items-center justify-center pointer-events-none">
                <div className="bg-[#24283b] border border-[#414868] rounded-lg p-6 shadow-xl text-center max-w-md mx-4 pointer-events-auto">
                  <h3 className="text-lg font-semibold text-[#c0caf5] mb-2">
                    Session Closed
                  </h3>
                  <p className="text-[#a9b1d6] mb-4">
                    This conversation can be resumed with Claude.
                  </p>
                  <button
                    onClick={handleResume}
                    disabled={isResuming}
                    className="px-6 py-2 bg-[#7aa2f7] hover:bg-[#89b4fa] text-[#1a1b26] rounded-lg font-medium disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                  >
                    {isResuming ? 'Resuming...' : 'Resume Session'}
                  </button>
                </div>
              </div>
            )}
          </div>
        ) : (
          // Show terminal for active sessions
          <Terminal
            sessionId={session.id}
            onData={handleTerminalData}
          />
        )}
      </div>

      {/* Typing indicator for multi-device sync */}
      {session.status === 'active' && (
        <TypingIndicator sessionId={session.id} currentSenderId="desktop" />
      )}

      {/* Status bar */}
      <footer className="flex items-center justify-between px-3 py-1 bg-[#16161e] border-t border-[#414868]/50 text-xs text-[#565f89] font-mono">
        <div className="flex items-center gap-4">
          <span>Session: {session.id.slice(0, 8)}</span>
          {session.conversationId && (
            <span>Conv: {session.conversationId.slice(0, 8)}</span>
          )}
        </div>
        <div className="flex items-center gap-4">
          <span>esc: interrupt</span>
          <span>?: help</span>
        </div>
      </footer>
    </div>
  );
}

// Message bubble component for displaying chat history
function MessageBubble({ message }: { message: ClaudeMessage }) {
  const isUser = message.role === 'user';

  // Truncate very long messages for display
  const displayContent =
    message.content.length > 2000
      ? message.content.slice(0, 2000) + '...'
      : message.content;

  return (
    <div className="font-mono text-sm">
      {/* Role indicator */}
      <div className={`flex items-center gap-2 mb-1 ${isUser ? 'text-[#7aa2f7]' : 'text-[#bb9af7]'}`}>
        <span className="text-xs">{isUser ? '❯' : '●'}</span>
        <span className="text-xs font-medium">{isUser ? 'You' : 'Claude'}</span>
        {message.timestamp && (
          <span className="text-[#414868] text-xs ml-auto">
            {new Date(message.timestamp).toLocaleTimeString()}
          </span>
        )}
      </div>
      {/* Message content */}
      <div className={`pl-4 ${isUser ? 'text-[#c0caf5]' : 'text-[#a9b1d6]'} whitespace-pre-wrap break-words leading-relaxed`}>
        {displayContent}
      </div>
    </div>
  );
}

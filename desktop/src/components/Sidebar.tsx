import { useState, useEffect, useRef } from 'react';
import { clsx } from 'clsx';
import { open } from '@tauri-apps/plugin-dialog';
import { Session, WaitingState, useSessionStore } from '../hooks/useSession';

// CLI icons/badges
const CLI_BADGES: Record<string, { label: string; color: string }> = {
  claude: { label: 'C', color: 'bg-orange-500' },
  gemini: { label: 'G', color: 'bg-blue-500' },
  codex: { label: 'X', color: 'bg-green-500' },
  opencode: { label: 'O', color: 'bg-indigo-500' },
};

// Context menu state
interface ContextMenuState {
  x: number;
  y: number;
  session: Session;
}

interface SidebarProps {
  sessions: Session[];
  activeSessionId: string | null;
  onSelectSession: (sessionId: string | null) => void;
  onOpenSettings?: () => void;
  onOpenAbout?: () => void;
  onOpenHelp?: () => void;
}

export default function Sidebar({
  sessions,
  activeSessionId,
  onSelectSession,
  onOpenSettings,
  onOpenAbout,
  onOpenHelp,
}: SidebarProps) {
  const { createSession, resumeSession, closeSession, deleteSession, fetchAvailableClis, availableClis, waitingStates } = useSessionStore();
  const [isCreating, setIsCreating] = useState(false);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);

  // Close context menu when clicking outside
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (contextMenuRef.current && !contextMenuRef.current.contains(e.target as Node)) {
        setContextMenu(null);
      }
    };

    if (contextMenu) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [contextMenu]);

  // Handle context menu actions
  const handleCloseSession = async () => {
    if (!contextMenu) return;
    try {
      await closeSession(contextMenu.session.id);
      if (activeSessionId === contextMenu.session.id) {
        onSelectSession(null);
      }
    } catch (e) {
      console.error('Failed to close session:', e);
    }
    setContextMenu(null);
  };

  const handleDeleteSession = async () => {
    if (!contextMenu) return;
    if (!confirm(`Delete session "${contextMenu.session.name}"? This cannot be undone.`)) {
      setContextMenu(null);
      return;
    }
    try {
      await deleteSession(contextMenu.session.id);
      if (activeSessionId === contextMenu.session.id) {
        onSelectSession(null);
      }
    } catch (e) {
      console.error('Failed to delete session:', e);
      alert('Failed to delete session: ' + String(e));
    }
    setContextMenu(null);
  };

  const handleContextMenu = (e: React.MouseEvent, session: Session) => {
    e.preventDefault();
    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      session,
    });
  };

  // Fetch available CLIs on mount
  useEffect(() => {
    fetchAvailableClis();
  }, [fetchAvailableClis]);

  const handleNewSession = async () => {
    console.log('New Session button clicked');
    try {
      // Open folder picker dialog
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Project Folder',
      });

      console.log('Dialog result:', selected);

      if (!selected) return; // User cancelled

      setIsCreating(true);

      // Always use the default CLI from settings (no picker)
      const installedClis = availableClis.filter(cli => cli.installed);
      const savedDefault = localStorage.getItem('defaultCli');
      const cliType = installedClis.find(c => c.id === savedDefault)?.id
        || installedClis[0]?.id
        || 'claude';

      await createSession(selected as string, undefined, cliType);
      setIsCreating(false);
    } catch (e) {
      console.error('Failed to create session:', e);
      alert('Error: ' + String(e));
      setIsCreating(false);
    }
  };

  const activeSessions = sessions.filter((s) => s.status !== 'closed');
  const closedSessions = sessions.filter((s) => s.status === 'closed');

  return (
    <aside className={`${isCollapsed ? 'w-14' : 'w-64'} bg-[#16161e] border-r border-[#414868]/50 flex flex-col transition-all duration-200`}>
      {/* Header */}
      <div className="p-3 border-b border-[#414868]/50 flex items-center justify-between">
        {!isCollapsed && (
          <div className="min-w-0 flex-1">
            <h1 className="text-lg font-semibold text-[#c0caf5]">
              MobileCLI
            </h1>
            <p className="text-xs text-[#565f89] mt-0.5">
              AI CLI Sessions
            </p>
          </div>
        )}
        <button
          onClick={() => setIsCollapsed(!isCollapsed)}
          className="p-1.5 rounded hover:bg-[#24283b] text-[#565f89] flex-shrink-0"
          title={isCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            {isCollapsed ? (
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 5l7 7-7 7M5 5l7 7-7 7" />
            ) : (
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 19l-7-7 7-7m8 14l-7-7 7-7" />
            )}
          </svg>
        </button>
      </div>

      {/* New Session */}
      <div className="p-3 border-b border-[#414868]/50">
        <button
          onClick={handleNewSession}
          disabled={isCreating}
          className={`${isCollapsed ? 'w-8 h-8 p-0' : 'w-full'} bg-[#7aa2f7] hover:bg-[#89b4fa] text-[#1a1b26] font-medium rounded-lg text-sm disabled:opacity-50 flex items-center justify-center gap-2 py-2 px-4 transition-colors`}
          title="New Session"
        >
          {isCreating ? (
            <span className="animate-spin">⏳</span>
          ) : isCollapsed ? (
            <span className="text-lg">+</span>
          ) : (
            <>
              <span>+</span>
              New Session
            </>
          )}
        </button>
      </div>

      {/* Session List */}
      <div className="flex-1 overflow-y-auto">
        {/* Active Sessions */}
        {activeSessions.length > 0 && (
          <div className="p-2">
            {!isCollapsed && (
              <h3 className="text-xs font-medium text-[#565f89] px-2 mb-1">
                Active
              </h3>
            )}
            {activeSessions.map((session) => (
              <SessionItem
                key={session.id}
                session={session}
                isActive={session.id === activeSessionId}
                onClick={() => onSelectSession(session.id)}
                onContextMenu={handleContextMenu}
                isCollapsed={isCollapsed}
                waitingState={waitingStates[session.id]}
                isHistory={false}
              />
            ))}
          </div>
        )}

        {/* Closed Sessions */}
        {closedSessions.length > 0 && (
          <div className="p-2 border-t border-[#414868]/50">
            {!isCollapsed && (
              <h3 className="text-xs font-medium text-[#565f89] px-2 mb-1">
                History
              </h3>
            )}
            {closedSessions.slice(0, 10).map((session) => (
              <SessionItem
                key={session.id}
                session={session}
                isActive={session.id === activeSessionId}
                onClick={() => onSelectSession(session.id)}
                onResume={session.conversationId ? () => resumeSession(session.id) : undefined}
                onContextMenu={handleContextMenu}
                isCollapsed={isCollapsed}
                isHistory={true}
              />
            ))}
          </div>
        )}

        {sessions.length === 0 && !isCollapsed && (
          <div className="p-4 text-center text-[#565f89] text-sm">
            No sessions yet.
            <br />
            Click "New Session" to get started.
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="p-3 border-t border-[#414868]/50 text-xs text-[#565f89]">
        <div className={`flex items-center ${isCollapsed ? 'flex-col gap-2' : 'justify-between'}`}>
          <div className={`flex items-center ${isCollapsed ? 'justify-center' : 'gap-2'}`} title="WebSocket: Connected">
            <span className="w-2 h-2 rounded-full bg-[#9ece6a] flex-shrink-0"></span>
            {!isCollapsed && <span>WebSocket: Connected</span>}
          </div>
          <div className={`flex items-center ${isCollapsed ? 'flex-col gap-1' : 'gap-1'}`}>
            <button
              onClick={onOpenHelp}
              className="p-1.5 rounded hover:bg-[#24283b] text-[#565f89] hover:text-[#a9b1d6] transition-colors"
              title="Help & FAQ"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
            </button>
            <button
              onClick={onOpenAbout}
              className="p-1.5 rounded hover:bg-[#24283b] text-[#565f89] hover:text-[#a9b1d6] transition-colors"
              title="About MobileCLI"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
            </button>
            <button
              onClick={onOpenSettings}
              className="p-1.5 rounded hover:bg-[#24283b] text-[#565f89] hover:text-[#a9b1d6] transition-colors"
              title="Connection Settings"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
            </button>
          </div>
        </div>
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          ref={contextMenuRef}
          className="fixed bg-[#1a1b26] border border-[#414868] rounded-lg shadow-xl py-1 z-50 min-w-[140px]"
          style={{ top: contextMenu.y, left: contextMenu.x }}
        >
          <div className="px-3 py-1.5 text-xs text-[#565f89] border-b border-[#414868]/50 truncate max-w-[180px]">
            {contextMenu.session.name}
          </div>
          {contextMenu.session.status !== 'closed' ? (
            <button
              onClick={handleCloseSession}
              className="w-full text-left px-3 py-2 text-sm text-[#c0caf5] hover:bg-[#24283b] flex items-center gap-2 transition-colors"
            >
              <svg className="w-4 h-4 text-[#e0af68]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
              Close Session
            </button>
          ) : (
            <button
              onClick={handleDeleteSession}
              className="w-full text-left px-3 py-2 text-sm text-[#f7768e] hover:bg-[#24283b] flex items-center gap-2 transition-colors"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
              Delete Session
            </button>
          )}
        </div>
      )}
    </aside>
  );
}

interface SessionItemProps {
  session: Session;
  isActive: boolean;
  onClick: () => void;
  onResume?: () => void;
  onContextMenu?: (e: React.MouseEvent, session: Session) => void;
  isCollapsed?: boolean;
  waitingState?: WaitingState;
  isHistory?: boolean;
}

function SessionItem({ session, isActive, onClick, onResume, onContextMenu, isCollapsed, waitingState, isHistory }: SessionItemProps) {
  const [isResuming, setIsResuming] = useState(false);

  // Determine display state based on waiting state and session status
  // Priority: tool_approval > awaiting_response > working > completed
  let displayState: 'working' | 'awaiting_approval' | 'awaiting_response' | 'completed' | 'history' = 'completed';

  if (isHistory || session.status === 'closed') {
    displayState = 'history';
  } else if (waitingState?.waitType === 'tool_approval') {
    displayState = 'awaiting_approval';
  } else if (waitingState?.waitType === 'awaiting_response') {
    displayState = 'awaiting_response';
  } else if (session.status === 'active') {
    // Active with no waiting state = Claude is working
    displayState = 'working';
  } else {
    // idle status = completed/paused
    displayState = 'completed';
  }

  // Status colors and text
  const statusConfig = {
    working: { color: 'bg-[#9ece6a]', text: 'Claude working...' },
    awaiting_approval: { color: 'bg-[#e0af68]', text: 'Awaiting approval' },
    awaiting_response: { color: 'bg-[#e0af68]', text: 'Awaiting response' },
    completed: { color: 'bg-[#565f89]', text: 'Completed' },
    history: { color: '', text: '' }, // No dot for history
  };

  const config = statusConfig[displayState];

  // Extract project name from path
  const projectName = session.projectPath.split('/').pop() || session.projectPath;

  // Get CLI badge
  const cliBadge = CLI_BADGES[session.cliType] || CLI_BADGES.claude;

  const handleResume = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!onResume) return;
    setIsResuming(true);
    try {
      await onResume();
    } catch (err) {
      console.error('Failed to resume:', err);
    } finally {
      setIsResuming(false);
    }
  };

  if (isCollapsed) {
    return (
      <div
        onClick={onClick}
        onContextMenu={onContextMenu ? (e) => onContextMenu(e, session) : undefined}
        className={clsx(
          'w-8 h-8 mx-auto mb-1 rounded-lg flex items-center justify-center cursor-pointer relative',
          'hover:bg-[#24283b] transition-colors duration-150',
          isActive && 'bg-[#24283b]',
          isHistory && 'opacity-60'
        )}
        title={`${session.name} - ${projectName} (${session.cliType})${config.text ? ` - ${config.text}` : ''} (Right-click for options)`}
      >
        <span className={clsx('w-5 h-5 rounded text-white text-xs font-bold flex items-center justify-center', cliBadge.color)}>
          {cliBadge.label}
        </span>
        {config.color && (
          <span className={clsx('absolute top-0.5 right-0.5 w-1.5 h-1.5 rounded-full', config.color)} />
        )}
      </div>
    );
  }

  return (
    <div
      onClick={onClick}
      onContextMenu={onContextMenu ? (e) => onContextMenu(e, session) : undefined}
      className={clsx(
        'w-full text-left px-3 py-2 rounded-lg transition-colors duration-150 cursor-pointer',
        'hover:bg-[#24283b]',
        isActive && 'bg-[#24283b]',
        isHistory && 'opacity-60'
      )}
    >
      <div className="flex items-center gap-2">
        <span className={clsx('w-4 h-4 rounded text-white text-[10px] font-bold flex items-center justify-center flex-shrink-0', cliBadge.color)}>
          {cliBadge.label}
        </span>
        {/* Only show status dot for active sessions, not history */}
        {config.color ? (
          <span className={clsx('w-2 h-2 rounded-full flex-shrink-0', config.color)} />
        ) : (
          <span className="w-2 flex-shrink-0" /> // Placeholder for alignment
        )}
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium text-[#c0caf5] truncate">
            {session.name}
          </div>
          {/* Show status text for active sessions, project name for history */}
          {config.text ? (
            <div className={clsx('text-xs truncate', {
              'text-[#9ece6a]': displayState === 'working',
              'text-[#e0af68]': displayState === 'awaiting_approval' || displayState === 'awaiting_response',
              'text-[#565f89]': displayState === 'completed',
            })}>
              {config.text}
            </div>
          ) : (
            <div className="text-xs text-[#565f89] truncate">
              {projectName}
            </div>
          )}
        </div>
        {onResume && session.status === 'closed' && (
          <button
            onClick={handleResume}
            disabled={isResuming}
            className="px-2 py-1 text-xs bg-[#7aa2f7] hover:bg-[#89b4fa] text-[#1a1b26] rounded font-medium disabled:opacity-50 transition-colors"
            title="Resume this conversation"
          >
            {isResuming ? '...' : 'Resume'}
          </button>
        )}
      </div>
    </div>
  );
}

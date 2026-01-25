import { useState, useEffect, useRef } from 'react';
import { clsx } from 'clsx';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { Session, WaitingState, InputState, useSessionStore } from '../hooks/useSession';

// CLI icons/badges
const CLI_BADGES: Record<string, { label: string; color: string }> = {
  claude: { label: 'C', color: 'bg-orange-500' },
  gemini: { label: 'G', color: 'bg-blue-500' },
  codex: { label: 'X', color: 'bg-green-500' },
  opencode: { label: 'O', color: 'bg-indigo-500' },
};

// CLI display names for status text
const CLI_NAMES: Record<string, string> = {
  claude: 'Claude',
  gemini: 'Gemini',
  codex: 'Codex',
  opencode: 'OpenCode',
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
  const { createSession, resumeSession, closeSession, renameSession, deleteSession, fetchAvailableClis, availableClis, waitingStates, inputStates } = useSessionStore();
  const [isCreating, setIsCreating] = useState(false);
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [showCollapsedIcons, setShowCollapsedIcons] = useState(() => {
    const saved = localStorage.getItem('sidebarShowCollapsedIcons');
    return saved !== 'false';
  });
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);
  // ISSUE #1: Create folder modal state
  const [showCreateFolderModal, setShowCreateFolderModal] = useState(false);
  const [parentPath, setParentPath] = useState('');
  const [newFolderName, setNewFolderName] = useState('');
  const [createError, setCreateError] = useState<string | null>(null);
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [showRenameModal, setShowRenameModal] = useState(false);
  const [renameTarget, setRenameTarget] = useState<Session | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [renameError, setRenameError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);

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

  useEffect(() => {
    const handleToggle = (event: Event) => {
      const detail = (event as CustomEvent<boolean>).detail;
      if (typeof detail === 'boolean') {
        setShowCollapsedIcons(detail);
      }
    };
    window.addEventListener('sidebar-collapsed-icons', handleToggle as EventListener);
    return () => {
      window.removeEventListener('sidebar-collapsed-icons', handleToggle as EventListener);
    };
  }, []);

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

  const handleOpenRename = () => {
    if (!contextMenu) return;
    setRenameTarget(contextMenu.session);
    setRenameValue(contextMenu.session.name);
    setRenameError(null);
    setShowRenameModal(true);
    setContextMenu(null);
  };

  const handleRenameConfirm = async () => {
    if (!renameTarget) return;
    const name = renameValue.trim();
    if (!name) {
      setRenameError('Session name cannot be empty');
      return;
    }
    if (name === renameTarget.name) {
      setShowRenameModal(false);
      setRenameTarget(null);
      return;
    }
    setIsRenaming(true);
    setRenameError(null);
    try {
      await renameSession(renameTarget.id, name);
      setShowRenameModal(false);
      setRenameTarget(null);
    } catch (e) {
      setRenameError(String(e));
    } finally {
      setIsRenaming(false);
    }
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

  // ISSUE #1: Browse for parent folder
  const handleBrowseParent = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Parent Folder',
      });
      if (selected) {
        setParentPath(selected as string);
        setCreateError(null);
      }
    } catch (e) {
      console.error('Failed to select parent folder:', e);
    }
  };

  // ISSUE #1: Create new folder and start session
  const handleCreateFolderAndSession = async () => {
    if (!parentPath || !newFolderName.trim()) return;

    // Validate folder name
    const name = newFolderName.trim();
    if (/[/\\:*?"<>|]/.test(name)) {
      setCreateError('Folder name contains invalid characters');
      return;
    }

    setIsCreatingFolder(true);
    setCreateError(null);

    try {
      // Build full path
      const fullPath = parentPath.endsWith('/') || parentPath.endsWith('\\')
        ? `${parentPath}${name}`
        : `${parentPath}/${name}`;

      // Create the directory
      await invoke('create_directory', { path: fullPath });

      // Close modal
      setShowCreateFolderModal(false);
      setParentPath('');
      setNewFolderName('');

      // Create session in the new folder
      setIsCreating(true);
      const installedClis = availableClis.filter(cli => cli.installed);
      const savedDefault = localStorage.getItem('defaultCli');
      const cliType = installedClis.find(c => c.id === savedDefault)?.id
        || installedClis[0]?.id
        || 'claude';

      await createSession(fullPath, undefined, cliType);
    } catch (e) {
      console.error('Failed to create folder:', e);
      setCreateError(String(e));
    } finally {
      setIsCreatingFolder(false);
      setIsCreating(false);
    }
  };

  const handleNewSession = async () => {
    try {
      // Open folder picker dialog
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Project Folder',
      });

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
        <div className={`${isCollapsed ? '' : 'flex gap-2'}`}>
          <button
            onClick={handleNewSession}
            disabled={isCreating}
            className={`${isCollapsed ? 'w-8 h-8 p-0 mb-2' : 'flex-1'} bg-[#7aa2f7] hover:bg-[#89b4fa] text-[#1a1b26] font-medium rounded-lg text-sm disabled:opacity-50 flex items-center justify-center gap-2 py-2 px-3 transition-colors`}
            title="Select existing folder"
          >
            {isCreating ? (
              <span className="animate-spin">⏳</span>
            ) : isCollapsed ? (
              <span className="text-lg">+</span>
            ) : (
              <>
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                </svg>
                Open
              </>
            )}
          </button>
          {/* ISSUE #1: Create new folder button */}
          <button
            onClick={() => setShowCreateFolderModal(true)}
            disabled={isCreating}
            className={`${isCollapsed ? 'w-8 h-8 p-0' : ''} bg-[#9ece6a] hover:bg-[#a9d974] text-[#1a1b26] font-medium rounded-lg text-sm disabled:opacity-50 flex items-center justify-center gap-2 py-2 px-3 transition-colors`}
            title="Create new folder"
          >
            {isCollapsed ? (
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 13h6m-3-3v6m-9 1V7a2 2 0 012-2h6l2 2h6a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z" />
              </svg>
            ) : (
              <>
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 13h6m-3-3v6m-9 1V7a2 2 0 012-2h6l2 2h6a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z" />
                </svg>
                New
              </>
            )}
          </button>
        </div>
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
                showCollapsedIcons={showCollapsedIcons}
                waitingState={waitingStates[session.id]}
                inputState={inputStates[session.id]} // ISSUE #5: Pass input state for typing indicator
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
                showCollapsedIcons={showCollapsedIcons}
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
            <a
              href="https://discord.gg/xu9RZkGDwf"
              target="_blank"
              rel="noopener noreferrer"
              className="p-1.5 rounded hover:bg-[#24283b] text-[#565f89] hover:text-[#a9b1d6] transition-colors"
              title="Join Discord"
              aria-label="Join Discord"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M20.317 4.369a19.791 19.791 0 00-4.885-1.515.074.074 0 00-.079.037c-.211.375-.444.864-.608 1.249-1.844-.276-3.68-.276-5.486 0-.164-.399-.405-.874-.617-1.249a.077.077 0 00-.079-.037 19.736 19.736 0 00-4.885 1.515.069.069 0 00-.032.027C.533 9.045-.32 13.579.099 18.057a.082.082 0 00.031.056 19.912 19.912 0 006.017 3.057.077.077 0 00.084-.027c.462-.63.874-1.295 1.226-1.994a.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128c.126-.094.252-.192.371-.291a.074.074 0 01.077-.01c3.927 1.793 8.18 1.793 12.061 0a.074.074 0 01.078.009c.119.099.245.198.372.292a.077.077 0 01-.006.127 12.299 12.299 0 01-1.873.892.077.077 0 00-.04.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.867 19.867 0 006.017-3.057.077.077 0 00.031-.056c.5-5.177-.838-9.673-3.548-13.66a.061.061 0 00-.031-.028zM8.02 15.331c-1.183 0-2.156-1.085-2.156-2.419 0-1.333.955-2.418 2.156-2.418 1.21 0 2.175 1.095 2.156 2.418 0 1.334-.955 2.419-2.156 2.419zm7.975 0c-1.183 0-2.156-1.085-2.156-2.419 0-1.333.955-2.418 2.156-2.418 1.21 0 2.175 1.095 2.156 2.418 0 1.334-.946 2.419-2.156 2.419z" />
              </svg>
            </a>
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
          <button
            onClick={handleOpenRename}
            className="w-full text-left px-3 py-2 text-sm text-[#c0caf5] hover:bg-[#24283b] flex items-center gap-2 transition-colors"
          >
            <svg className="w-4 h-4 text-[#7aa2f7]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M16.862 3.487a2.25 2.25 0 013.182 3.182L8.25 18.463l-4.5 1.125 1.125-4.5L16.862 3.487z" />
            </svg>
            Rename
          </button>
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

      {/* ISSUE #1: Create Folder Modal */}
      {showCreateFolderModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-[#1a1b26] border border-[#414868] rounded-lg p-6 w-96 shadow-xl">
            <h2 className="text-lg font-semibold text-[#c0caf5] mb-4">Create New Folder</h2>

            {/* Parent folder selection */}
            <div className="mb-4">
              <label className="block text-sm text-[#a9b1d6] mb-2">Parent Folder</label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={parentPath}
                  onChange={(e) => setParentPath(e.target.value)}
                  placeholder="Select or enter parent path..."
                  className="flex-1 bg-[#24283b] border border-[#414868] rounded px-3 py-2 text-sm text-[#c0caf5] placeholder-[#565f89] focus:outline-none focus:border-[#7aa2f7]"
                />
                <button
                  onClick={handleBrowseParent}
                  className="px-3 py-2 bg-[#414868] hover:bg-[#565f89] text-[#c0caf5] rounded text-sm transition-colors"
                >
                  Browse
                </button>
              </div>
            </div>

            {/* New folder name */}
            <div className="mb-4">
              <label className="block text-sm text-[#a9b1d6] mb-2">Folder Name</label>
              <input
                type="text"
                value={newFolderName}
                onChange={(e) => {
                  setNewFolderName(e.target.value);
                  setCreateError(null);
                }}
                placeholder="my-new-project"
                className="w-full bg-[#24283b] border border-[#414868] rounded px-3 py-2 text-sm text-[#c0caf5] placeholder-[#565f89] focus:outline-none focus:border-[#7aa2f7]"
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && parentPath && newFolderName.trim()) {
                    handleCreateFolderAndSession();
                  }
                }}
                autoFocus
              />
            </div>

            {/* Error message */}
            {createError && (
              <div className="mb-4 p-2 bg-[#f7768e]/20 border border-[#f7768e]/50 rounded text-sm text-[#f7768e]">
                {createError}
              </div>
            )}

            {/* Preview path */}
            {parentPath && newFolderName && (
              <div className="mb-4 p-2 bg-[#24283b] rounded text-xs text-[#565f89] font-mono truncate">
                {parentPath.endsWith('/') || parentPath.endsWith('\\')
                  ? `${parentPath}${newFolderName.trim()}`
                  : `${parentPath}/${newFolderName.trim()}`}
              </div>
            )}

            {/* Buttons */}
            <div className="flex gap-3 justify-end">
              <button
                onClick={() => {
                  setShowCreateFolderModal(false);
                  setParentPath('');
                  setNewFolderName('');
                  setCreateError(null);
                }}
                className="px-4 py-2 text-sm text-[#a9b1d6] hover:text-[#c0caf5] transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleCreateFolderAndSession}
                disabled={!parentPath || !newFolderName.trim() || isCreatingFolder}
                className="px-4 py-2 bg-[#9ece6a] hover:bg-[#a9d974] text-[#1a1b26] font-medium rounded text-sm disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                {isCreatingFolder ? 'Creating...' : 'Create & Start Session'}
              </button>
            </div>
          </div>
        </div>
      )}

      {showRenameModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-[#1a1b26] border border-[#414868] rounded-lg p-6 w-96 shadow-xl">
            <h2 className="text-lg font-semibold text-[#c0caf5] mb-4">Rename Session</h2>

            <div className="mb-4">
              <label className="block text-sm text-[#a9b1d6] mb-2">Session Name</label>
              <input
                type="text"
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') handleRenameConfirm();
                  if (e.key === 'Escape') {
                    setShowRenameModal(false);
                    setRenameTarget(null);
                  }
                }}
                className="w-full bg-[#24283b] border border-[#414868] rounded px-3 py-2 text-sm text-[#c0caf5] placeholder-[#565f89] focus:outline-none focus:border-[#7aa2f7]"
                placeholder="Enter session name..."
              />
              {renameError && (
                <p className="text-xs text-[#f7768e] mt-2">{renameError}</p>
              )}
            </div>

            <div className="flex justify-end gap-2">
              <button
                onClick={() => {
                  setShowRenameModal(false);
                  setRenameTarget(null);
                }}
                className="px-4 py-2 text-sm text-[#a9b1d6] hover:text-[#c0caf5] transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleRenameConfirm}
                disabled={isRenaming}
                className="px-4 py-2 bg-[#7aa2f7] hover:bg-[#89b4fa] text-[#1a1b26] font-medium rounded text-sm disabled:opacity-50 transition-colors"
              >
                {isRenaming ? 'Renaming...' : 'Rename'}
              </button>
            </div>
          </div>
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
  showCollapsedIcons?: boolean;
  waitingState?: WaitingState;
  inputState?: InputState; // ISSUE #5: For "User typing" indicator
  isHistory?: boolean;
}

function SessionItem({ session, isActive, onClick, onResume, onContextMenu, isCollapsed, showCollapsedIcons, waitingState, inputState, isHistory }: SessionItemProps) {
  const [isResuming, setIsResuming] = useState(false);

  // ISSUE #5: Check if user is typing (input buffer has content)
  const isUserTyping = inputState && inputState.text && inputState.text.length > 0;

  // Determine display state based on waiting state, input state, and session status
  // Priority: history > tool_approval > user_typing > awaiting_response > working > completed
  // ISSUE #5: user_typing beats awaiting_response - "Prevent 'awaiting response' from appearing during typing"
  let displayState: 'user_typing' | 'working' | 'awaiting_approval' | 'awaiting_response' | 'completed' | 'history' | 'clarifying' | 'plan_approval' = 'completed';

  if (isHistory || session.status === 'closed') {
    displayState = 'history';
  } else if (waitingState?.waitType === 'tool_approval') {
    // Tool approval always takes priority - user must respond
    displayState = 'awaiting_approval';
  } else if (waitingState?.waitType === 'plan_approval') {
    displayState = 'plan_approval';
  } else if (waitingState?.waitType === 'clarifying_question') {
    displayState = 'clarifying';
  } else if (isUserTyping) {
    // ISSUE #5: User is typing - show "User typing" instead of "working" or "awaiting response"
    displayState = 'user_typing';
  } else if (waitingState?.waitType === 'awaiting_response') {
    displayState = 'awaiting_response';
  } else if (session.status === 'active') {
    // Active with no waiting state and not typing = Claude is working
    displayState = 'working';
  } else {
    // idle status = completed/paused
    displayState = 'completed';
  }

  // Get CLI display name
  const cliName = CLI_NAMES[session.cliType] || 'CLI';

  // Status colors and text
  const statusConfig = {
    user_typing: { color: 'bg-[#7aa2f7]', text: 'User typing...' }, // ISSUE #5: Blue for user typing
    working: { color: 'bg-[#9ece6a]', text: `${cliName} working...` },
    awaiting_approval: { color: 'bg-[#e0af68]', text: 'Awaiting approval' },
    awaiting_response: { color: 'bg-[#e0af68]', text: 'Awaiting response' },
    plan_approval: { color: 'bg-[#e0af68]', text: 'Plan approval' },
    clarifying: { color: 'bg-[#e0af68]', text: 'Question pending' },
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
    if (showCollapsedIcons === false) {
      return null;
    }
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
              'text-[#7aa2f7]': displayState === 'user_typing', // ISSUE #5: Blue for user typing
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

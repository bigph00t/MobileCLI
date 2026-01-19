import { useEffect, useState } from 'react';
import { listen, emit } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import Sidebar from './components/Sidebar';
import ChatView from './components/ChatView';
import SettingsPanel from './components/SettingsPanel';
import { SetupWizard } from './components/setup';
import { ClientView } from './components/ClientView';
import { AboutScreen } from './components/AboutScreen';
import { HelpScreen } from './components/HelpScreen';
import { ErrorBoundary } from './components/ErrorBoundary';
import { useSessionStore } from './hooks/useSession';
import { useConfigStore } from './hooks/useConfig';
import { writeToTerminal } from './components/Terminal';

function App() {
  const {
    sessions,
    activeSessionId,
    setActiveSession,
    fetchSessions,
    createSession,
    closeSession,
  } = useSessionStore();

  const {
    config,
    isLoading: configLoading,
    fetchConfig,
    setFirstRunComplete,
    setAppMode,
  } = useConfigStore();

  const [isLoading, setIsLoading] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const [showWizard, setShowWizard] = useState(false);
  const [showAbout, setShowAbout] = useState(false);
  const [showHelp, setShowHelp] = useState(false);

  // Update window title with version
  useEffect(() => {
    invoke<string>('get_version').then((version) => {
      document.title = `MobileCLI ${version}`;
    }).catch(() => {
      document.title = 'MobileCLI';
    });
  }, []);

  useEffect(() => {
    // Load config and sessions on mount
    const init = async () => {
      await fetchConfig();
      await fetchSessions();
      setIsLoading(false);
    };
    init();

    // Listen for PTY output - route to terminal
    const unlistenPty = listen<{
      sessionId: string;
      output: string;
      raw: string;
    }>('pty-output', (event) => {
      const { sessionId, raw } = event.payload;
      // Write raw output (with ANSI codes) to terminal
      writeToTerminal(sessionId, raw);
    });

    // Listen for session-created events (from mobile or other sources)
    const unlistenSessionCreated = listen<{
      id: string;
      name: string;
      projectPath: string;
      createdAt: string;
      lastActiveAt: string;
      status: string;
      cliType: string;
    }>('session-created', () => {
      // Refetch sessions to get the new session
      fetchSessions();
    });

    // Listen for session-resumed events (from mobile)
    const unlistenSessionResumed = listen<{
      id: string;
      name: string;
      projectPath: string;
      createdAt: string;
      lastActiveAt: string;
      status: string;
      cliType: string;
    }>('session-resumed', () => {
      // Refetch sessions to get updated status
      fetchSessions();
    });

    // Listen for session-closed events (from mobile)
    const unlistenSessionClosed = listen<{
      sessionId: string;
    }>('session-closed', () => {
      // Refetch sessions to get updated status
      fetchSessions();
    });

    // Listen for session-renamed events (from mobile)
    const unlistenSessionRenamed = listen<{
      sessionId: string;
      newName: string;
    }>('session-renamed', (event) => {
      const { sessionId, newName } = event.payload;
      // Update the session name in the store directly
      useSessionStore.setState((state) => ({
        sessions: state.sessions.map((s) =>
          s.id === sessionId ? { ...s, name: newName } : s
        ),
      }));
    });

    // Listen for session-deleted events (from mobile)
    const unlistenSessionDeleted = listen<{
      sessionId: string;
    }>('session-deleted', (event) => {
      const { sessionId } = event.payload;
      // Remove the session from the store
      useSessionStore.setState((state) => ({
        sessions: state.sessions.filter((s) => s.id !== sessionId),
        activeSessionId: state.activeSessionId === sessionId ? null : state.activeSessionId,
      }));
    });

    // Listen for waiting-for-input events (tool approval, awaiting response)
    const unlistenWaitingForInput = listen<{
      sessionId: string;
      timestamp: string;
      promptContent?: string;
    }>('waiting-for-input', (event) => {
      const { sessionId, timestamp, promptContent } = event.payload;
      // Detect if this is a tool approval based on prompt content
      const isToolApproval = promptContent?.includes('Do you want to') ||
        promptContent?.includes('Allow') ||
        promptContent?.includes('approve') ||
        promptContent?.includes('[Y/n]') ||
        promptContent?.includes('(y/n)');

      useSessionStore.getState().setWaitingState(sessionId, {
        sessionId,
        waitType: isToolApproval ? 'tool_approval' : 'awaiting_response',
        promptContent,
        timestamp,
      });
    });

    // Listen for waiting-cleared events (tool approval resolved)
    const unlistenWaitingCleared = listen<{
      sessionId: string;
    }>('waiting-cleared', (event) => {
      const { sessionId } = event.payload;
      useSessionStore.getState().setWaitingState(sessionId, null);
    });

    // Listen for assistant messages (clears working state)
    const unlistenMessage = listen<{
      sessionId: string;
      role: string;
    }>('message', (event) => {
      const { sessionId, role } = event.payload;
      // When assistant sends a message, clear waiting state (they're done)
      if (role === 'assistant') {
        const currentState = useSessionStore.getState().waitingStates[sessionId];
        // Only clear if not in tool_approval state (that should stay until user responds)
        if (currentState?.waitType !== 'tool_approval') {
          useSessionStore.getState().setWaitingState(sessionId, null);
        }
      }
    });

    // ISSUE #5: Listen for input-state events (from Terminal.tsx or mobile via WS)
    // This tracks when user is typing to show "User typing" instead of "Claude working"
    const unlistenInputState = listen<{
      sessionId: string;
      text: string;
      cursorPosition: number;
      timestamp: number;
    }>('input-state', (event) => {
      const { sessionId, text, cursorPosition, timestamp } = event.payload;
      if (text && text.length > 0) {
        useSessionStore.getState().setInputState(sessionId, {
          text,
          cursorPosition,
          timestamp,
        });
      } else {
        // Clear input state when text is empty
        useSessionStore.getState().setInputState(sessionId, null);
      }
    });

    // FIX FOR ISSUE 1 & 6: Listen for waiting state requests from mobile
    // When mobile subscribes to a session, send the current waiting state
    const unlistenRequestWaitingState = listen<{
      sessionId: string;
    }>('request-waiting-state', async (event) => {
      const { sessionId } = event.payload;
      const waitingState = useSessionStore.getState().waitingStates[sessionId];

      console.log('[App] Received request-waiting-state for session', sessionId, 'current state:', waitingState);

      // Emit the current waiting state so mobile can sync
      // If there's a waiting state, emit it as waiting-for-input event
      // If no waiting state, emit waiting-cleared to ensure mobile is in sync
      if (waitingState && waitingState.waitType) {
        await emit('waiting-for-input', {
          sessionId,
          timestamp: waitingState.timestamp,
          promptContent: waitingState.promptContent || '',
        });
        console.log('[App] Sent waiting-for-input to mobile for session', sessionId);
      } else {
        // No waiting state - send cleared to ensure mobile shows correct status
        await emit('waiting-cleared', {
          sessionId,
          timestamp: new Date().toISOString(),
        });
        console.log('[App] Sent waiting-cleared to mobile for session', sessionId);
      }
    });

    return () => {
      unlistenPty.then((fn) => fn());
      unlistenSessionCreated.then((fn) => fn());
      unlistenSessionResumed.then((fn) => fn());
      unlistenSessionClosed.then((fn) => fn());
      unlistenSessionRenamed.then((fn) => fn());
      unlistenSessionDeleted.then((fn) => fn());
      unlistenWaitingForInput.then((fn) => fn());
      unlistenWaitingCleared.then((fn) => fn());
      unlistenMessage.then((fn) => fn());
      unlistenInputState.then((fn) => fn()); // ISSUE #5
      unlistenRequestWaitingState.then((fn) => fn());
    };
  }, []);

  // Show setup wizard on first run
  useEffect(() => {
    if (!isLoading && config?.firstRun) {
      setShowWizard(true);
    }
  }, [isLoading, config?.firstRun]);

  const handleWizardComplete = async (selectedMode: 'host' | 'client') => {
    await setAppMode(selectedMode);
    await setFirstRunComplete();
    setShowWizard(false);
  };

  const activeSession = sessions.find((s) => s.id === activeSessionId);

  const handleCloseSession = async () => {
    if (activeSessionId) {
      await closeSession(activeSessionId);
      setActiveSession(null);
    }
  };

  if (isLoading || configLoading) {
    return (
      <div className="h-screen flex items-center justify-center bg-[#1a1b26]">
        <div className="text-[#565f89] font-mono animate-pulse">Loading...</div>
      </div>
    );
  }

  // Show setup wizard on first run
  if (showWizard) {
    return <SetupWizard onComplete={handleWizardComplete} />;
  }

  // Client mode: show client view instead of host UI
  if (config?.mode === 'client') {
    return <ClientView />;
  }

  // Host mode: show full host UI
  return (
    <div className="h-screen flex bg-[#1a1b26]">
      {/* Sidebar */}
      <Sidebar
        sessions={sessions}
        activeSessionId={activeSessionId}
        onSelectSession={setActiveSession}
        onOpenSettings={() => setShowSettings(true)}
        onOpenAbout={() => setShowAbout(true)}
        onOpenHelp={() => setShowHelp(true)}
      />

      {/* Settings Panel */}
      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}

      {/* About Screen */}
      {showAbout && <AboutScreen onClose={() => setShowAbout(false)} />}

      {/* Help Screen */}
      {showHelp && <HelpScreen onClose={() => setShowHelp(false)} />}

      {/* Main content */}
      <main className="flex-1 flex flex-col min-w-0">
        {activeSession ? (
          <ChatView session={activeSession} onClose={handleCloseSession} />
        ) : (
          <div className="flex-1 flex items-center justify-center bg-[#1a1b26]">
            <div className="text-center">
              <div className="text-4xl mb-4 text-[#7aa2f7]">❯_</div>
              <h2 className="text-xl font-semibold text-[#c0caf5] mb-2">
                Welcome to MobileCLI
              </h2>
              <p className="text-[#565f89] mb-6 max-w-md">
                Control AI coding assistants from anywhere. Create a session to start working.
              </p>
              <button
                onClick={async () => {
                  const selected = await open({
                    directory: true,
                    multiple: false,
                    title: 'Select Project Folder',
                  });
                  if (selected) {
                    // Use default CLI from settings
                    const savedDefault = localStorage.getItem('defaultCli');
                    await createSession(selected as string, undefined, savedDefault || 'claude');
                  }
                }}
                className="px-6 py-2 bg-[#7aa2f7] hover:bg-[#89b4fa] text-[#1a1b26] rounded-lg font-medium transition-colors"
              >
                New Session
              </button>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}

function AppWithErrorBoundary() {
  return (
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  );
}

export default AppWithErrorBoundary;

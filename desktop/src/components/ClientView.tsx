import { useEffect, useState } from 'react';
import { useClientSync, SessionInfo, Activity } from '../hooks/useClientSync';
import { useConfig } from '../hooks/useConfig';

interface ClientConnectScreenProps {
  onConnect: (relayUrl: string, roomCode: string, key: string) => Promise<void>;
  error: string | null;
  connecting: boolean;
}

function ClientConnectScreen({ onConnect, error, connecting }: ClientConnectScreenProps) {
  const { config } = useConfig();
  const [hostUrl, setHostUrl] = useState(config?.lastHostUrl || '');
  const [roomCode, setRoomCode] = useState(config?.lastRoomCode || '');
  const [encryptionKey, setEncryptionKey] = useState('');

  const handleConnect = async () => {
    if (!hostUrl.trim() || !roomCode.trim() || !encryptionKey.trim()) {
      return;
    }
    await onConnect(hostUrl.trim(), roomCode.trim(), encryptionKey.trim());
  };

  return (
    <div className="flex-1 flex items-center justify-center bg-[#1a1b26]">
      <div className="max-w-md w-full p-6">
        <div className="text-center mb-8">
          <div className="w-16 h-16 mx-auto mb-4 bg-blue-600 rounded-2xl flex items-center justify-center">
            <svg className="w-8 h-8 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0" />
            </svg>
          </div>
          <h1 className="text-xl font-bold text-white mb-2">Connect to Host</h1>
          <p className="text-gray-400 text-sm">
            Enter the connection details from your host computer
          </p>
        </div>

        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">
              Relay URL
            </label>
            <input
              type="text"
              value={hostUrl}
              onChange={(e) => setHostUrl(e.target.value)}
              placeholder="wss://relay.mobilecli.app"
              className="w-full bg-gray-700 text-white px-4 py-3 rounded-lg border border-gray-600 focus:border-blue-500 focus:outline-none"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">
              Room Code
            </label>
            <input
              type="text"
              value={roomCode}
              onChange={(e) => setRoomCode(e.target.value)}
              placeholder="abc123"
              className="w-full bg-gray-700 text-white px-4 py-3 rounded-lg border border-gray-600 focus:border-blue-500 focus:outline-none"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-300 mb-1">
              Encryption Key
            </label>
            <input
              type="password"
              value={encryptionKey}
              onChange={(e) => setEncryptionKey(e.target.value)}
              placeholder="Base64 encoded key"
              className="w-full bg-gray-700 text-white px-4 py-3 rounded-lg border border-gray-600 focus:border-blue-500 focus:outline-none"
            />
            <p className="text-xs text-gray-500 mt-1">
              Scan the QR code from your host, or enter the key manually
            </p>
          </div>

          {error && (
            <div className="bg-red-900/50 text-red-400 px-4 py-2 rounded-lg text-sm">
              {error}
            </div>
          )}

          <button
            onClick={handleConnect}
            disabled={connecting || !hostUrl.trim() || !roomCode.trim() || !encryptionKey.trim()}
            className="w-full bg-blue-600 text-white px-6 py-3 rounded-lg font-medium hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {connecting ? 'Connecting...' : 'Connect'}
          </button>
        </div>

        <div className="mt-6 pt-4 border-t border-gray-700">
          <div className="flex items-start gap-2 text-sm text-gray-400">
            <svg className="w-5 h-5 text-blue-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
            <span>
              On the host computer, go to Settings → Connection → Show QR Code to get the connection details.
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}

interface ClientSessionListProps {
  sessions: SessionInfo[];
  selected: string | null;
  onSelect: (id: string) => void;
}

function ClientSessionList({ sessions, selected, onSelect }: ClientSessionListProps) {
  return (
    <div className="w-64 bg-[#1f2937] border-r border-gray-700 flex flex-col">
      <div className="p-4 border-b border-gray-700">
        <h2 className="font-semibold text-white">Sessions</h2>
        <p className="text-xs text-gray-400 mt-1">{sessions.length} from host</p>
      </div>

      <div className="flex-1 overflow-y-auto">
        {sessions.length === 0 ? (
          <div className="p-4 text-gray-500 text-sm text-center">
            No active sessions on host
          </div>
        ) : (
          sessions.map((session) => (
            <button
              key={session.id}
              onClick={() => onSelect(session.id)}
              className={`w-full p-4 text-left hover:bg-gray-700/50 transition-colors ${
                selected === session.id ? 'bg-gray-700' : ''
              }`}
            >
              <div className="flex items-center gap-2 mb-1">
                <span className={`w-2 h-2 rounded-full ${
                  session.status === 'active' ? 'bg-green-500' : 'bg-gray-500'
                }`} />
                <span className="font-medium text-white truncate">{session.name}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className={`text-xs px-1.5 py-0.5 rounded ${
                  session.cliType === 'claude' ? 'bg-orange-900/50 text-orange-400' : 'bg-blue-900/50 text-blue-400'
                }`}>
                  {session.cliType === 'claude' ? 'Claude' : 'Gemini'}
                </span>
                <span className="text-xs text-gray-500 truncate">
                  {session.projectPath.split('/').pop()}
                </span>
              </div>
            </button>
          ))
        )}
      </div>
    </div>
  );
}

interface ClientChatViewProps {
  session: SessionInfo;
  activities: Activity[];
  onSendInput: (text: string) => void;
}

function ClientChatView({ session, activities, onSendInput }: ClientChatViewProps) {
  const [input, setInput] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (input.trim()) {
      onSendInput(input);
      setInput('');
    }
  };

  return (
    <div className="flex-1 flex flex-col bg-[#1a1b26]">
      {/* Header */}
      <div className="p-4 border-b border-gray-700 flex items-center justify-between">
        <div>
          <h2 className="font-semibold text-white">{session.name}</h2>
          <p className="text-xs text-gray-400">{session.projectPath}</p>
        </div>
        <span className={`px-2 py-1 rounded text-xs ${
          session.status === 'active' ? 'bg-green-900/50 text-green-400' : 'bg-gray-700 text-gray-400'
        }`}>
          {session.status}
        </span>
      </div>

      {/* Activity feed */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {activities.length === 0 ? (
          <div className="text-center text-gray-500 mt-8">
            <p>No activities yet</p>
            <p className="text-sm mt-1">Activities from this session will appear here</p>
          </div>
        ) : (
          activities.map((activity) => (
            <div key={activity.id} className="bg-gray-800/50 rounded-lg p-4">
              <div className="flex items-center gap-2 mb-2">
                <span className={`text-xs px-2 py-0.5 rounded ${
                  activity.type === 'user' ? 'bg-blue-900/50 text-blue-400' :
                  activity.type === 'assistant' ? 'bg-purple-900/50 text-purple-400' :
                  activity.type === 'tool' ? 'bg-yellow-900/50 text-yellow-400' :
                  'bg-gray-700 text-gray-400'
                }`}>
                  {activity.type}
                </span>
                <span className="text-xs text-gray-500">{activity.timestamp}</span>
              </div>
              <div className="text-gray-200 text-sm whitespace-pre-wrap font-mono">
                {activity.content}
              </div>
            </div>
          ))
        )}
      </div>

      {/* Input */}
      <form onSubmit={handleSubmit} className="p-4 border-t border-gray-700">
        <div className="flex gap-2">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Send a message to the session..."
            className="flex-1 bg-gray-700 text-white px-4 py-2 rounded-lg border border-gray-600 focus:border-blue-500 focus:outline-none"
          />
          <button
            type="submit"
            disabled={!input.trim()}
            className="bg-blue-600 text-white px-4 py-2 rounded-lg hover:bg-blue-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Send
          </button>
        </div>
      </form>
    </div>
  );
}

export function ClientView() {
  const {
    sessions,
    activities,
    connected,
    connecting,
    error,
    pendingApprovals,
    connect,
    disconnect,
    subscribeToSession,
    sendInput,
    sendToolApproval,
    clearError,
  } = useClientSync();

  const [selectedSession, setSelectedSession] = useState<string | null>(null);

  // Subscribe to selected session
  useEffect(() => {
    if (selectedSession && connected) {
      subscribeToSession(selectedSession);
    }
  }, [selectedSession, connected, subscribeToSession]);

  // Select first session by default
  useEffect(() => {
    if (sessions.length > 0 && !selectedSession) {
      setSelectedSession(sessions[0].id);
    }
  }, [sessions, selectedSession]);

  if (!connected) {
    return (
      <ClientConnectScreen
        onConnect={connect}
        error={error}
        connecting={connecting}
      />
    );
  }

  const selectedSessionData = sessions.find((s) => s.id === selectedSession);
  const sessionActivities = selectedSession ? activities.get(selectedSession) || [] : [];

  // Get pending approvals for selected session
  const sessionApprovals = pendingApprovals.filter((a) => a.sessionId === selectedSession);

  return (
    <div className="relative flex h-screen bg-[#1a1b26]">
      <ClientSessionList
        sessions={sessions}
        selected={selectedSession}
        onSelect={setSelectedSession}
      />

      {selectedSessionData ? (
        <div className="flex-1 flex flex-col">
          <ClientChatView
            session={selectedSessionData}
            activities={sessionActivities}
            onSendInput={(text) => sendInput(selectedSession!, text)}
          />

          {/* Tool approval modal */}
          {sessionApprovals.length > 0 && (
            <div className="absolute inset-0 bg-black/50 flex items-center justify-center">
              <div className="bg-gray-800 rounded-lg p-6 max-w-md w-full mx-4">
                <h3 className="text-lg font-bold text-white mb-2">Tool Approval Required</h3>
                <div className="bg-gray-900 rounded p-4 mb-4">
                  <div className="text-yellow-400 font-mono text-sm mb-2">
                    {sessionApprovals[0].toolName}
                  </div>
                  <pre className="text-gray-400 text-xs overflow-x-auto">
                    {JSON.stringify(sessionApprovals[0].params, null, 2)}
                  </pre>
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={() => sendToolApproval(selectedSession!, sessionApprovals[0].approvalId, true)}
                    className="flex-1 bg-green-600 text-white px-4 py-2 rounded hover:bg-green-700"
                  >
                    Approve
                  </button>
                  <button
                    onClick={() => sendToolApproval(selectedSession!, sessionApprovals[0].approvalId, true, true)}
                    className="flex-1 bg-blue-600 text-white px-4 py-2 rounded hover:bg-blue-700"
                  >
                    Always
                  </button>
                  <button
                    onClick={() => sendToolApproval(selectedSession!, sessionApprovals[0].approvalId, false)}
                    className="flex-1 bg-red-600 text-white px-4 py-2 rounded hover:bg-red-700"
                  >
                    Deny
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-gray-500">
          <div className="text-center">
            <svg className="w-16 h-16 mx-auto mb-4 text-gray-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
            </svg>
            <p className="text-lg font-medium">Select a session</p>
            <p className="text-sm mt-1">Choose a session from the list to view activities</p>
          </div>
        </div>
      )}

      {/* Disconnect button */}
      <button
        onClick={disconnect}
        className="absolute top-4 right-4 text-gray-400 hover:text-white transition-colors"
        title="Disconnect"
      >
        <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
        </svg>
      </button>

      <a
        href="https://discord.gg/xu9RZkGDwf"
        target="_blank"
        rel="noopener noreferrer"
        className="absolute bottom-4 left-4 p-2 rounded-lg bg-[#1a1b26]/80 border border-[#414868]/60 text-[#565f89] hover:text-[#a9b1d6] hover:border-[#565f89] transition-colors"
        title="Join Discord"
        aria-label="Join Discord"
      >
        <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
          <path d="M20.317 4.369a19.791 19.791 0 00-4.885-1.515.074.074 0 00-.079.037c-.211.375-.444.864-.608 1.249-1.844-.276-3.68-.276-5.486 0-.164-.399-.405-.874-.617-1.249a.077.077 0 00-.079-.037 19.736 19.736 0 00-4.885 1.515.069.069 0 00-.032.027C.533 9.045-.32 13.579.099 18.057a.082.082 0 00.031.056 19.912 19.912 0 006.017 3.057.077.077 0 00.084-.027c.462-.63.874-1.295 1.226-1.994a.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128c.126-.094.252-.192.371-.291a.074.074 0 01.077-.01c3.927 1.793 8.18 1.793 12.061 0a.074.074 0 01.078.009c.119.099.245.198.372.292a.077.077 0 01-.006.127 12.299 12.299 0 01-1.873.892.077.077 0 00-.04.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.867 19.867 0 006.017-3.057.077.077 0 00.031-.056c.5-5.177-.838-9.673-3.548-13.66a.061.061 0 00-.031-.028zM8.02 15.331c-1.183 0-2.156-1.085-2.156-2.419 0-1.333.955-2.418 2.156-2.418 1.21 0 2.175 1.095 2.156 2.418 0 1.334-.955 2.419-2.156 2.419zm7.975 0c-1.183 0-2.156-1.085-2.156-2.419 0-1.333.955-2.418 2.156-2.418 1.21 0 2.175 1.095 2.156 2.418 0 1.334-.946 2.419-2.156 2.419z" />
        </svg>
      </a>

      {/* Error toast */}
      {error && (
        <div className="absolute bottom-4 right-4 bg-red-900 text-red-200 px-4 py-2 rounded-lg shadow-lg flex items-center gap-2">
          <span>{error}</span>
          <button onClick={clearError} className="text-red-400 hover:text-red-300">
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      )}
    </div>
  );
}

export default ClientView;

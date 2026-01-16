import { useState, useEffect } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useSessionStore } from '../hooks/useSession';
import { getCurrentTerminalTheme, setTerminalTheme, TERMINAL_THEMES, type TerminalThemeName } from './Terminal';

interface SettingsPanelProps {
  onClose: () => void;
}

interface RelayQrData {
  url: string;
  roomCode: string;
  key: string;
  connected: boolean;
}

interface TailscaleStatus {
  installed: boolean;
  running: boolean;
  tailscaleIp: string | null;
  hostname: string | null;
  wsUrl: string | null;
}

type RelayConnectionStatus = 'connected' | 'reconnecting' | 'disconnected';

// CLI badge colors
const CLI_BADGES: Record<string, { label: string; color: string }> = {
  claude: { label: 'C', color: 'bg-orange-500' },
  gemini: { label: 'G', color: 'bg-blue-500' },
};

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [localIp, setLocalIp] = useState<string>('');
  const [wsPort] = useState(9847);
  const [wsReady, setWsReady] = useState<boolean>(false);
  const [wsError, setWsError] = useState<string | null>(null);
  const [defaultCli, setDefaultCli] = useState<string>('claude');
  const [activeTab, setActiveTab] = useState<'local' | 'relay' | 'tailscale' | 'appearance'>('local');
  const [terminalTheme, setCurrentTerminalTheme] = useState<TerminalThemeName>(getCurrentTerminalTheme());
  const [relay, setRelay] = useState<RelayQrData | null>(null);
  const [relayStarting, setRelayStarting] = useState(false);
  const [relayError, setRelayError] = useState<string | null>(null);
  const [relayStatus, setRelayStatus] = useState<RelayConnectionStatus>('disconnected');
  const [customRelayUrl, setCustomRelayUrl] = useState<string>('');
  const [showAdvancedRelay, setShowAdvancedRelay] = useState(false);
  const [tailscale, setTailscale] = useState<TailscaleStatus | null>(null);
  const { availableClis, fetchAvailableClis } = useSessionStore();

  useEffect(() => {
    // Check if WS server is ready
    checkWsReady();
    // Get local IP address
    getLocalIp();
    // Check existing relay status
    checkRelayStatus();
    // Fetch available CLIs
    fetchAvailableClis();
    // Check Tailscale status
    getTailscaleStatus();
    // Load saved default CLI
    const saved = localStorage.getItem('defaultCli');
    if (saved) setDefaultCli(saved);

    // Load custom relay URL from config
    loadCustomRelayUrl();

    // Listen for WS server ready event
    const unlistenReady = listen('ws-server-ready', () => {
      setWsReady(true);
      setWsError(null);
    });

    // Listen for WS server error event
    const unlistenError = listen<{ error: string }>('ws-server-error', (event) => {
      setWsError(event.payload.error);
    });

    // Listen for relay events
    const unlistenRelayConnected = listen('relay-client-connected', () => {
      setRelay(prev => prev ? { ...prev, connected: true } : null);
    });

    const unlistenRelayDisconnected = listen('relay-client-disconnected', () => {
      setRelay(prev => prev ? { ...prev, connected: false } : null);
    });

    const unlistenRelayError = listen<string>('relay-error', (event) => {
      setRelayError(event.payload);
    });

    const unlistenRelayClose = listen('relay-disconnected', () => {
      setRelay(null);
      setRelayStatus('disconnected');
    });

    // Listen for relay status changes (connected/reconnecting/disconnected)
    const unlistenRelayStatus = listen<RelayConnectionStatus>('relay-status', (event) => {
      setRelayStatus(event.payload);
    });

    return () => {
      unlistenReady.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenRelayConnected.then((fn) => fn());
      unlistenRelayDisconnected.then((fn) => fn());
      unlistenRelayError.then((fn) => fn());
      unlistenRelayClose.then((fn) => fn());
      unlistenRelayStatus.then((fn) => fn());
    };
  }, [fetchAvailableClis]);

  const checkWsReady = async () => {
    try {
      const ready = await invoke<boolean>('is_ws_ready');
      setWsReady(ready);
    } catch (e) {
      console.error('Failed to check WS ready:', e);
    }
  };

  const getLocalIp = async () => {
    try {
      // Try to get IP from Tauri backend
      const ip = await invoke<string>('get_local_ip');
      setLocalIp(ip);
    } catch (e) {
      console.error('Failed to get local IP:', e);
      setLocalIp('localhost');
    }
  };

  const checkRelayStatus = async () => {
    try {
      const status = await invoke<RelayQrData | null>('get_relay_status');
      setRelay(status);
    } catch (e) {
      console.error('Failed to get relay status:', e);
    }
  };

  const getTailscaleStatus = async () => {
    try {
      const status = await invoke<TailscaleStatus>('get_tailscale_status');
      setTailscale(status);
    } catch (e) {
      console.error('Failed to get tailscale status:', e);
      setTailscale(null);
    }
  };

  const startRelay = async () => {
    setRelayStarting(true);
    setRelayError(null);
    try {
      const qrData = await invoke<RelayQrData>('start_relay');
      setRelay(qrData);
    } catch (e) {
      console.error('Failed to start relay:', e);
      setRelayError(e instanceof Error ? e.message : String(e));
    } finally {
      setRelayStarting(false);
    }
  };

  const stopRelay = async () => {
    try {
      await invoke('stop_relay');
      setRelay(null);
      setRelayError(null);
    } catch (e) {
      console.error('Failed to stop relay:', e);
    }
  };

  // Generate relay QR code data (includes room code and encryption key)
  // Uses URL format that mobile expects: mobilecli://relay?url=...&room=...&key=...
const getRelayQrValue = () => {
  if (!relay) return '';
  const params = new URLSearchParams({
    url: relay.url,
    room: relay.roomCode,
    key: relay.key,
  });
  return `mobilecli://relay?${params.toString()}`;
};

const getTailscaleQrValue = () => {
  if (!tailscale?.wsUrl) return '';
  const params = new URLSearchParams({ url: tailscale.wsUrl });
  return `mobilecli://tailscale?${params.toString()}`;
};


  const handleDefaultCliChange = (cliId: string) => {
    setDefaultCli(cliId);
    localStorage.setItem('defaultCli', cliId);
  };

  const handleThemeChange = (theme: TerminalThemeName) => {
    setTerminalTheme(theme);
    setCurrentTerminalTheme(theme);
  };

  // Load custom relay URL from config
  const loadCustomRelayUrl = async () => {
    try {
        const config = await invoke<{ relay_urls: string[] }>('get_config');
        if (config.relay_urls && config.relay_urls.length > 0) {
          const url = config.relay_urls[0];
          if (url !== 'wss://relay.mobilecli.app') {
            setCustomRelayUrl(url);
            setShowAdvancedRelay(true);
          }
        }

    } catch (e) {
      console.error('Failed to load config:', e);
    }
  };

  // Save custom relay URL to config
  const saveCustomRelayUrl = async (url: string) => {
    try {
      const config = await invoke<Record<string, unknown>>('get_config');
      const updatedConfig = {
        ...config,
        relay_urls: url.trim() ? [url.trim()] : ['wss://relay.mobilecli.app'],
      };
      await invoke('set_config', { config: updatedConfig });
      setCustomRelayUrl(url);
    } catch (e) {
      console.error('Failed to save config:', e);
    }
  };

  const wsUrl = `ws://${localIp}:${wsPort}`;
  const installedClis = availableClis.filter(cli => cli.installed);

  return (
    <div className="fixed inset-0 bg-[#1a1b26]/80 backdrop-blur-sm flex items-center justify-center z-50 p-4">
      <div className="bg-[#24283b] border border-[#414868] rounded-xl shadow-xl max-w-md w-full max-h-[90vh] flex flex-col overflow-hidden">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#414868]">
          <h2 className="text-lg font-semibold text-[#c0caf5]">
            Settings
          </h2>
          <button
            onClick={onClose}
            className="p-1 text-[#565f89] hover:text-[#c0caf5] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-[#414868]">
          <button
            onClick={() => setActiveTab('local')}
            className={`flex-1 px-3 py-3 text-sm font-medium transition-colors ${
              activeTab === 'local'
                ? 'text-[#7aa2f7] border-b-2 border-[#7aa2f7]'
                : 'text-[#565f89] hover:text-[#a9b1d6]'
            }`}
          >
            Local
          </button>
          <button
            onClick={() => setActiveTab('relay')}
            className={`flex-1 px-3 py-3 text-sm font-medium transition-colors flex items-center justify-center gap-1 ${
              activeTab === 'relay'
                ? 'text-[#7aa2f7] border-b-2 border-[#7aa2f7]'
                : 'text-[#565f89] hover:text-[#a9b1d6]'
            }`}
          >
            Relay
            {relay && (
              <span className={`w-2 h-2 rounded-full ${
                relayStatus === 'connected' && relay.connected
                  ? 'bg-green-500'
                  : relayStatus === 'reconnecting'
                  ? 'bg-yellow-500 animate-pulse'
                  : relayStatus === 'connected'
                  ? 'bg-blue-500'
                  : 'bg-red-500'
              }`} />
            )}
          </button>
          <button
            onClick={() => setActiveTab('tailscale')}
            className={`flex-1 px-3 py-3 text-sm font-medium transition-colors ${
              activeTab === 'tailscale'
                ? 'text-[#7aa2f7] border-b-2 border-[#7aa2f7]'
                : 'text-[#565f89] hover:text-[#a9b1d6]'
            }`}
          >
            Tailscale
          </button>
          <button
            onClick={() => setActiveTab('appearance')}
            className={`flex-1 px-3 py-3 text-sm font-medium transition-colors ${
              activeTab === 'appearance'
                ? 'text-[#7aa2f7] border-b-2 border-[#7aa2f7]'
                : 'text-[#565f89] hover:text-[#a9b1d6]'
            }`}
          >
            Appearance
          </button>
        </div>

        {/* Content - Scrollable */}
        <div className="p-6 overflow-y-auto flex-1">
          {activeTab === 'local' ? (
            <>
              {/* Local Network QR Code */}
              <div className="flex flex-col items-center mb-6">
                {!wsReady ? (
                  <>
                    {wsError ? (
                      <>
                        <div className="w-16 h-16 rounded-full bg-red-500/20 flex items-center justify-center mb-4">
                          <svg className="w-8 h-8 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                          </svg>
                        </div>
                        <p className="text-sm text-red-400 text-center">{wsError}</p>
                      </>
                    ) : (
                      <>
                        <div className="w-16 h-16 rounded-full bg-[#1a1b26] flex items-center justify-center mb-4">
                          <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-[#7aa2f7]"></div>
                        </div>
                        <p className="text-sm text-[#565f89] text-center">Starting server...</p>
                      </>
                    )}
                  </>
                ) : (
                  <>
                    {localIp === 'localhost' ? (
                      <>
                        {/* Warning: localhost cannot be used for mobile connection */}
                        <div className="w-16 h-16 rounded-full bg-yellow-500/20 flex items-center justify-center mb-4">
                          <svg className="w-8 h-8 text-yellow-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                          </svg>
                        </div>
                        <p className="text-sm text-yellow-400 text-center mb-2">
                          No local network IP detected
                        </p>
                        <p className="text-xs text-[#565f89] text-center mb-4">
                          Your mobile device cannot connect via "localhost". Make sure your computer is connected to WiFi or use the Relay tab for remote connections.
                        </p>
                        <div className="bg-[#1a1b26] p-3 rounded-lg border border-yellow-500/30">
                          <p className="text-xs font-mono text-[#565f89] text-center">
                            {wsUrl}
                          </p>
                        </div>
                      </>
                    ) : (
                      <>
                        <p className="text-sm text-[#a9b1d6] mb-4 text-center">
                          Scan this QR code with the MobileCLI app to connect on your local network
                        </p>
                        <div className="bg-white p-4 rounded-lg">
                          <QRCodeCanvas
                            value={wsUrl}
                            size={200}
                            level="M"
                            bgColor="#ffffff"
                            fgColor="#1a1b26"
                          />
                        </div>
                      </>
                    )}
                  </>
                )}
              </div>

              {/* CLI Preferences - Local Tab Only */}
              {installedClis.length > 0 && (
                <div className="mb-6 p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                  <h3 className="text-sm font-medium text-[#c0caf5] mb-3">
                    Default CLI
                  </h3>
                  <p className="text-xs text-[#565f89] mb-3">
                    Select which CLI to use when creating new sessions
                  </p>
                  <div className="space-y-2">
                    {installedClis.map(cli => {
                      const badge = CLI_BADGES[cli.id] || { label: '?', color: 'bg-gray-500' };
                      const isSelected = defaultCli === cli.id;
                      return (
                        <button
                          key={cli.id}
                          onClick={() => handleDefaultCliChange(cli.id)}
                          className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg border transition-colors ${
                            isSelected
                              ? 'border-[#7aa2f7] bg-[#7aa2f7]/10'
                              : 'border-[#414868]/50 hover:bg-[#24283b]'
                          }`}
                        >
                          <span className={`w-6 h-6 rounded text-white text-xs font-bold flex items-center justify-center ${badge.color}`}>
                            {badge.label}
                          </span>
                          <span className="flex-1 text-left text-sm text-[#c0caf5]">
                            {cli.name}
                          </span>
                          {!cli.supportsResume && (
                            <span className="text-xs text-[#565f89]">(no resume)</span>
                          )}
                          {isSelected && (
                            <svg className="w-5 h-5 text-[#7aa2f7]" fill="currentColor" viewBox="0 0 20 20">
                              <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clipRule="evenodd" />
                            </svg>
                          )}
                        </button>
                      );
                    })}
                  </div>
                </div>
              )}

              {/* Connection Info - Local Tab */}
              <div className="space-y-4">
                <div>
                  <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                    WebSocket URL
                  </label>
                  <div className="mt-1 flex items-center gap-2">
                    <code className="flex-1 px-3 py-2 bg-[#1a1b26] border border-[#414868]/50 rounded text-sm font-mono text-[#7dcfff]">
                      {wsUrl}
                    </code>
                    <button
                      onClick={() => navigator.clipboard.writeText(wsUrl)}
                      className="px-3 py-2 bg-[#414868] hover:bg-[#565f89] rounded text-sm text-[#c0caf5] transition-colors"
                      title="Copy to clipboard"
                    >
                      Copy
                    </button>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                      Local IP
                    </label>
                    <p className="mt-1 text-sm font-mono text-[#c0caf5]">
                      {localIp || 'Detecting...'}
                    </p>
                  </div>
                  <div>
                    <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                      Port
                    </label>
                    <p className="mt-1 text-sm font-mono text-[#c0caf5]">
                      {wsPort}
                    </p>
                  </div>
                </div>
              </div>

              {/* Instructions - Local Tab */}
              <div className="mt-6 p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                <h3 className="text-sm font-medium text-[#c0caf5] mb-2">
                  How to Connect
                </h3>
                <ol className="text-xs text-[#a9b1d6] space-y-1 list-decimal list-inside">
                  <li>Open MobileCLI on your phone</li>
                  <li>Go to Settings tab</li>
                  <li>Tap "Scan QR Code to Connect"</li>
                  <li>Point your camera at the QR code above</li>
                </ol>
              </div>
            </>
          ) : activeTab === 'relay' ? (
            <>
              {/* Relay Connection */}
              {!relay ? (
                <div className="flex flex-col items-center mb-6">
                  <div className="w-16 h-16 rounded-full bg-[#1a1b26] flex items-center justify-center mb-4">
                    <svg className="w-8 h-8 text-[#bb9af7]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0" />
                    </svg>
                  </div>
                  <h3 className="text-[#c0caf5] font-medium mb-2">Relay Connection</h3>
                  <p className="text-sm text-[#565f89] text-center mb-4">
                    Connect from anywhere with end-to-end encryption through our relay server. No VPN or port forwarding needed.
                  </p>
                  {relayError && (
                    <p className="text-sm text-red-400 text-center mb-4">{relayError}</p>
                  )}

                  {/* Advanced Settings Toggle */}
                  <button
                    onClick={() => setShowAdvancedRelay(!showAdvancedRelay)}
                    className="text-xs text-[#565f89] hover:text-[#a9b1d6] mb-4 flex items-center gap-1 transition-colors"
                  >
                    <svg
                      className={`w-3 h-3 transition-transform ${showAdvancedRelay ? 'rotate-90' : ''}`}
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                    </svg>
                    Advanced Settings
                  </button>

                  {/* Custom Relay URL Input */}
                  {showAdvancedRelay && (
                    <div className="w-full mb-4 p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                      <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide block mb-2">
                        Custom Relay Server URL
                      </label>
                      <p className="text-xs text-[#565f89] mb-3">
                        Leave empty to use the default relay server
                      </p>
                      <div className="flex gap-2">
                        <input
                          type="text"
                          value={customRelayUrl}
                          onChange={(e) => setCustomRelayUrl(e.target.value)}
                          placeholder="wss://relay.mobilecli.app"
                          className="flex-1 px-3 py-2 bg-[#24283b] border border-[#414868] rounded text-sm text-[#c0caf5] placeholder-[#565f89] focus:outline-none focus:border-[#bb9af7]"
                        />
                        <button
                          onClick={() => saveCustomRelayUrl(customRelayUrl)}
                          className="px-3 py-2 bg-[#414868] hover:bg-[#565f89] rounded text-sm text-[#c0caf5] transition-colors"
                        >
                          Save
                        </button>
                      </div>
                      {customRelayUrl && (
                        <button
                          onClick={() => {
                            saveCustomRelayUrl('');
                            setCustomRelayUrl('');
                          }}
                          className="text-xs text-[#565f89] hover:text-red-400 mt-2 transition-colors"
                        >
                          Reset to default
                        </button>
                      )}
                    </div>
                  )}

                  <button
                    onClick={startRelay}
                    disabled={relayStarting}
                    className={`px-6 py-3 rounded-lg text-white text-sm font-medium transition-colors ${
                      relayStarting
                        ? 'bg-[#565f89] cursor-not-allowed'
                        : 'bg-[#bb9af7] hover:bg-[#bb9af7]/80'
                    }`}
                  >
                    {relayStarting ? (
                      <span className="flex items-center gap-2">
                        <div className="animate-spin rounded-full h-4 w-4 border-t-2 border-b-2 border-white"></div>
                        Connecting...
                      </span>
                    ) : (
                      'Start Relay Connection'
                    )}
                  </button>
                </div>
              ) : (
                <>
                  {/* Relay Server Status Bar */}
                  <div className="mb-4 p-3 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                        Relay Server
                      </span>
                      <div className="flex items-center gap-2">
                        <span className={`w-2 h-2 rounded-full ${
                          relayStatus === 'connected'
                            ? 'bg-green-500'
                            : relayStatus === 'reconnecting'
                            ? 'bg-yellow-500 animate-pulse'
                            : 'bg-red-500'
                        }`} />
                        <span className={`text-xs font-medium ${
                          relayStatus === 'connected'
                            ? 'text-green-400'
                            : relayStatus === 'reconnecting'
                            ? 'text-yellow-400'
                            : 'text-red-400'
                        }`}>
                          {relayStatus === 'connected'
                            ? 'Connected'
                            : relayStatus === 'reconnecting'
                            ? 'Reconnecting...'
                            : 'Disconnected'}
                        </span>
                      </div>
                    </div>
                  </div>

                  {/* Mobile Client Status */}
                  <div className="flex flex-col items-center mb-6">
                    <div className="flex items-center gap-2 mb-3">
                      <span className={`w-2 h-2 rounded-full ${relay.connected ? 'bg-green-500 animate-pulse' : 'bg-yellow-500'}`} />
                      <span className={`text-sm ${relay.connected ? 'text-green-400' : 'text-yellow-400'}`}>
                        {relay.connected ? 'Mobile Connected' : 'Waiting for Mobile'}
                      </span>
                    </div>
                    <p className="text-sm text-[#a9b1d6] mb-4 text-center">
                      Scan this QR code with MobileCLI to connect via encrypted relay
                    </p>
                    <div className="bg-white p-4 rounded-lg mb-4">
                      <QRCodeCanvas
                        value={getRelayQrValue()}
                        size={200}
                        level="M"
                        bgColor="#ffffff"
                        fgColor="#1a1b26"
                      />
                    </div>
                    <button
                      onClick={stopRelay}
                      className="px-4 py-2 bg-red-500/20 hover:bg-red-500/30 text-red-400 text-sm font-medium rounded-lg transition-colors"
                    >
                      Stop Relay
                    </button>
                  </div>

                  {/* Relay Connection Info */}
                  <div className="space-y-4">
                    <div>
                      <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                        Room Code
                      </label>
                      <div className="mt-1 flex items-center gap-2">
                        <code className="flex-1 px-3 py-2 bg-[#1a1b26] border border-[#414868]/50 rounded text-lg font-mono text-[#bb9af7] text-center tracking-widest">
                          {relay.roomCode}
                        </code>
                        <button
                          onClick={() => navigator.clipboard.writeText(relay.roomCode)}
                          className="px-3 py-2 bg-[#414868] hover:bg-[#565f89] rounded text-sm text-[#c0caf5] transition-colors"
                          title="Copy to clipboard"
                        >
                          Copy
                        </button>
                      </div>
                    </div>

                    <div>
                      <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                        Relay Server
                      </label>
                      <p className="mt-1 text-sm font-mono text-[#c0caf5]">
                        {relay.url}
                      </p>
                    </div>
                  </div>

                  {/* Relay Instructions */}
                  <div className="mt-6 p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                    <h3 className="text-sm font-medium text-[#c0caf5] mb-2">
                      How Relay Works
                    </h3>
                    <ul className="text-xs text-[#a9b1d6] space-y-1 list-disc list-inside">
                      <li>All data is end-to-end encrypted</li>
                      <li>The relay server only sees encrypted blobs</li>
                      <li>No VPN or port forwarding required</li>
                      <li>Works from any network worldwide</li>
                    </ul>
                  </div>

                  {/* Security Notice */}
                  <div className="mt-4 p-4 bg-[#bb9af7]/10 rounded-lg border border-[#bb9af7]/30">
                    <div className="flex items-start gap-2">
                      <svg className="w-5 h-5 text-[#bb9af7] flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                      </svg>
                      <div>
                        <h4 className="text-sm font-medium text-[#bb9af7]">End-to-End Encryption</h4>
                        <p className="text-xs text-[#a9b1d6] mt-1">
                          Your session data is encrypted with XSalsa20-Poly1305 before leaving your device. Only your phone with the QR code can decrypt it.
                        </p>
                      </div>
                    </div>
                  </div>
                </>
              )}
            </>
          ) : activeTab === 'tailscale' ? (
            <>
              <div className="flex flex-col items-center mb-6">
                <div className="w-16 h-16 rounded-full bg-[#1a1b26] flex items-center justify-center mb-4">
                  <svg className="w-8 h-8 text-[#7dcfff]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7l9-4 9 4-9 4-9-4zm0 6l9 4 9-4m-9 4v6" />
                  </svg>
                </div>
                <h3 className="text-[#c0caf5] font-medium mb-2">Tailscale Connection</h3>
                <p className="text-sm text-[#565f89] text-center mb-4">
                  Connect over your Tailnet with a private, encrypted tunnel.
                </p>
              </div>

              {tailscale?.running && tailscale.wsUrl ? (
                <>
                  <div className="mb-6 flex flex-col items-center">
                    <p className="text-sm text-[#a9b1d6] mb-4 text-center">
                      Scan this QR code with MobileCLI to connect via Tailscale
                    </p>
                    <div className="bg-white p-4 rounded-lg">
                      <QRCodeCanvas
                        value={getTailscaleQrValue()}
                        size={200}
                        level="M"
                        bgColor="#ffffff"
                        fgColor="#1a1b26"
                      />
                    </div>
                  </div>

                  <div className="space-y-4">
                    <div>
                      <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                        Tailscale WebSocket URL
                      </label>
                      <div className="mt-1 flex items-center gap-2">
                        <code className="flex-1 px-3 py-2 bg-[#1a1b26] border border-[#414868]/50 rounded text-sm font-mono text-[#7dcfff]">
                          {tailscale.wsUrl}
                        </code>
                        <button
                          onClick={() => navigator.clipboard.writeText(tailscale.wsUrl || '')}
                          className="px-3 py-2 bg-[#414868] hover:bg-[#565f89] rounded text-sm text-[#c0caf5] transition-colors"
                          title="Copy to clipboard"
                        >
                          Copy
                        </button>
                      </div>
                    </div>

                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                          Tailnet IP
                        </label>
                        <p className="mt-1 text-sm font-mono text-[#c0caf5]">
                          {tailscale.tailscaleIp || 'Unknown'}
                        </p>
                      </div>
                      <div>
                        <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                          Hostname
                        </label>
                        <p className="mt-1 text-sm font-mono text-[#c0caf5]">
                          {tailscale.hostname || 'Unknown'}
                        </p>
                      </div>
                    </div>
                  </div>

                  <div className="mt-6 p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                    <h3 className="text-sm font-medium text-[#c0caf5] mb-2">How to Connect</h3>
                    <ol className="text-xs text-[#a9b1d6] space-y-1 list-decimal list-inside">
                      <li>Join the same Tailnet on your phone</li>
                      <li>Open MobileCLI and scan the QR code</li>
                      <li>Stay on Tailscale for private access anywhere</li>
                    </ol>
                  </div>
                </>
              ) : (
                <div className="p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                  <p className="text-sm text-[#a9b1d6] mb-3">
                    {tailscale?.installed
                      ? 'Tailscale is installed but not running yet.'
                      : 'Tailscale is not installed on this machine yet.'}
                  </p>
                  <ul className="text-xs text-[#565f89] space-y-1 list-disc list-inside">
                    <li>Install and sign in to Tailscale</li>
                    <li>Make sure the desktop is connected to your Tailnet</li>
                    <li>Return here to scan the QR code</li>
                  </ul>
                  <button
                    onClick={() => window.open('https://tailscale.com/download', '_blank')}
                    className="mt-4 w-full px-4 py-2 bg-[#414868] hover:bg-[#565f89] rounded text-sm text-[#c0caf5] transition-colors"
                  >
                    Install Tailscale
                  </button>
                </div>
              )}
            </>
          ) : activeTab === 'appearance' ? (
            <>
              {/* Terminal Theme */}
              <div className="space-y-4">
                <div>
                  <label className="text-xs font-medium text-[#565f89] uppercase tracking-wide">
                    Terminal Theme
                  </label>
                  <p className="text-xs text-[#565f89] mt-1 mb-3">
                    Choose your preferred terminal appearance
                  </p>
                  <div className="grid grid-cols-3 gap-3">
                    {/* Classic Theme */}
                    <button
                      onClick={() => handleThemeChange('classic')}
                      className={`p-3 rounded-lg border transition-all ${
                        terminalTheme === 'classic'
                          ? 'border-[#7aa2f7] bg-[#1a1b26]'
                          : 'border-[#414868] bg-[#1a1b26] hover:border-[#565f89]'
                      }`}
                    >
                      <div
                        className="w-full h-16 rounded mb-2 border border-gray-700 flex items-center justify-center"
                        style={{ backgroundColor: TERMINAL_THEMES.classic.background }}
                      >
                        <span style={{ color: TERMINAL_THEMES.classic.foreground, fontFamily: 'monospace', fontSize: '10px' }}>
                          $ _
                        </span>
                      </div>
                      <span className={`text-xs font-medium ${
                        terminalTheme === 'classic' ? 'text-[#7aa2f7]' : 'text-[#a9b1d6]'
                      }`}>
                        Terminal
                      </span>
                    </button>

                    {/* Tokyo Night Theme */}
                    <button
                      onClick={() => handleThemeChange('tokyo-night')}
                      className={`p-3 rounded-lg border transition-all ${
                        terminalTheme === 'tokyo-night'
                          ? 'border-[#7aa2f7] bg-[#1a1b26]'
                          : 'border-[#414868] bg-[#1a1b26] hover:border-[#565f89]'
                      }`}
                    >
                      <div
                        className="w-full h-16 rounded mb-2 border border-gray-700 flex items-center justify-center"
                        style={{ backgroundColor: TERMINAL_THEMES['tokyo-night'].background }}
                      >
                        <span style={{ color: TERMINAL_THEMES['tokyo-night'].foreground, fontFamily: 'monospace', fontSize: '10px' }}>
                          $ _
                        </span>
                      </div>
                      <span className={`text-xs font-medium ${
                        terminalTheme === 'tokyo-night' ? 'text-[#7aa2f7]' : 'text-[#a9b1d6]'
                      }`}>
                        Tokyo Night
                      </span>
                    </button>

                    {/* Light Theme */}
                    <button
                      onClick={() => handleThemeChange('light')}
                      className={`p-3 rounded-lg border transition-all ${
                        terminalTheme === 'light'
                          ? 'border-[#7aa2f7] bg-[#1a1b26]'
                          : 'border-[#414868] bg-[#1a1b26] hover:border-[#565f89]'
                      }`}
                    >
                      <div
                        className="w-full h-16 rounded mb-2 border border-gray-300 flex items-center justify-center"
                        style={{ backgroundColor: TERMINAL_THEMES.light.background }}
                      >
                        <span style={{ color: TERMINAL_THEMES.light.foreground, fontFamily: 'monospace', fontSize: '10px' }}>
                          $ _
                        </span>
                      </div>
                      <span className={`text-xs font-medium ${
                        terminalTheme === 'light' ? 'text-[#7aa2f7]' : 'text-[#a9b1d6]'
                      }`}>
                        Light
                      </span>
                    </button>
                  </div>
                </div>

                {/* Theme Info */}
                <div className="mt-6 p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                  <h3 className="text-sm font-medium text-[#c0caf5] mb-2">
                    Theme Settings
                  </h3>
                  <ul className="text-xs text-[#a9b1d6] space-y-1 list-disc list-inside">
                    <li><strong>Terminal:</strong> Classic black & white terminal</li>
                    <li><strong>Tokyo Night:</strong> Dark blue theme with purple accents</li>
                    <li><strong>Light:</strong> Light mode for bright environments</li>
                  </ul>
                </div>
              </div>
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}

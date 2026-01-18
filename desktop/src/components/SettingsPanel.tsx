import { useState, useEffect } from 'react';
import { QRCodeCanvas } from 'qrcode.react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-shell';
import { useSessionStore } from '../hooks/useSession';
import { getCurrentTerminalTheme, setTerminalTheme, TERMINAL_THEMES, type TerminalThemeName } from './Terminal';

interface SettingsPanelProps {
  onClose: () => void;
}

interface TailscaleStatus {
  installed: boolean;
  running: boolean;
  tailscaleIp: string | null;
  hostname: string | null;
  wsUrl: string | null;
}

// CLI badge colors
const CLI_BADGES: Record<string, { label: string; color: string }> = {
  claude: { label: 'C', color: 'bg-orange-500' },
  gemini: { label: 'G', color: 'bg-blue-500' },
  codex: { label: 'X', color: 'bg-green-500' },
  opencode: { label: 'O', color: 'bg-indigo-500' },
};

export default function SettingsPanel({ onClose }: SettingsPanelProps) {
  const [localIp, setLocalIp] = useState<string>('');
  const [wsPort] = useState(9847);
  const [wsReady, setWsReady] = useState<boolean>(false);
  const [wsError, setWsError] = useState<string | null>(null);
  const [defaultCli, setDefaultCli] = useState<string>('claude');
  const [activeTab, setActiveTab] = useState<'general' | 'connectivity'>('general');
  const [connectionMethod, setConnectionMethod] = useState<'local' | 'tailscale'>('local');
  const [terminalTheme, setCurrentTerminalTheme] = useState<TerminalThemeName>(getCurrentTerminalTheme());
  const [tailscale, setTailscale] = useState<TailscaleStatus | null>(null);
  const { availableClis, fetchAvailableClis } = useSessionStore();

  useEffect(() => {
    // Check if WS server is ready
    checkWsReady();
    // Get local IP address
    getLocalIp();
    // Fetch available CLIs
    fetchAvailableClis();
    // Check Tailscale status
    getTailscaleStatus();
    // Load saved default CLI
    const saved = localStorage.getItem('defaultCli');
    if (saved) setDefaultCli(saved);

    // Listen for WS server ready event
    const unlistenReady = listen('ws-server-ready', () => {
      setWsReady(true);
      setWsError(null);
    });

    // Listen for WS server error event
    const unlistenError = listen<{ error: string }>('ws-server-error', (event) => {
      setWsError(event.payload.error);
    });

    return () => {
      unlistenReady.then((fn) => fn());
      unlistenError.then((fn) => fn());
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

  const getTailscaleStatus = async () => {
    try {
      const status = await invoke<TailscaleStatus>('get_tailscale_status');
      setTailscale(status);
    } catch (e) {
      console.error('Failed to get tailscale status:', e);
      setTailscale(null);
    }
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

  const wsUrl = `ws://${localIp}:${wsPort}`;

  return (
    <div className="fixed inset-0 bg-[#1a1b26]/80 backdrop-blur-sm flex items-center justify-center z-50 p-4">
      <div className="bg-[#24283b] border border-[#414868] rounded-xl shadow-xl max-w-lg w-full max-h-[90vh] flex flex-col overflow-hidden">
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

        {/* Main Tabs: General | Connectivity */}
        <div className="flex border-b border-[#414868]">
          <button
            onClick={() => setActiveTab('general')}
            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
              activeTab === 'general'
                ? 'text-[#7aa2f7] border-b-2 border-[#7aa2f7]'
                : 'text-[#565f89] hover:text-[#a9b1d6]'
            }`}
          >
            General
          </button>
          <button
            onClick={() => setActiveTab('connectivity')}
            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors flex items-center justify-center gap-2 ${
              activeTab === 'connectivity'
                ? 'text-[#7aa2f7] border-b-2 border-[#7aa2f7]'
                : 'text-[#565f89] hover:text-[#a9b1d6]'
            }`}
          >
            Connectivity
          </button>
        </div>

        {/* Content - Scrollable */}
        <div className="p-6 overflow-y-auto flex-1">
          {activeTab === 'general' ? (
            <>
              {/* Default CLI Selection */}
              {availableClis.length > 0 && (
                <div className="mb-6">
                  <h3 className="text-sm font-medium text-[#c0caf5] mb-2">
                    Default CLI
                  </h3>
                  <p className="text-xs text-[#565f89] mb-3">
                    Select which CLI to use when creating new sessions
                  </p>
                  <div className="space-y-2">
                    {availableClis.map(cli => {
                      const badge = CLI_BADGES[cli.id] || { label: '?', color: 'bg-gray-500' };
                      const isSelected = defaultCli === cli.id;
                      const isDisabled = !cli.installed;
                      return (
                        <button
                          key={cli.id}
                          onClick={() => cli.installed && handleDefaultCliChange(cli.id)}
                          disabled={isDisabled}
                          className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg border transition-colors ${
                            isDisabled
                              ? 'border-[#414868]/30 opacity-50 cursor-not-allowed'
                              : isSelected
                              ? 'border-[#7aa2f7] bg-[#7aa2f7]/10'
                              : 'border-[#414868]/50 hover:bg-[#1a1b26]'
                          }`}
                        >
                          <span className={`w-6 h-6 rounded text-white text-xs font-bold flex items-center justify-center ${isDisabled ? 'opacity-50' : ''} ${badge.color}`}>
                            {badge.label}
                          </span>
                          <span className={`flex-1 text-left text-sm ${isDisabled ? 'text-[#565f89]' : 'text-[#c0caf5]'}`}>
                            {cli.name}
                          </span>
                          {!cli.installed && (
                            <span className="text-xs text-[#f7768e]">(not installed)</span>
                          )}
                          {cli.installed && !cli.supportsResume && (
                            <span className="text-xs text-[#565f89]">(no resume)</span>
                          )}
                          {isSelected && cli.installed && (
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

              {/* Terminal Theme */}
              <div className="mb-6">
                <h3 className="text-sm font-medium text-[#c0caf5] mb-2">
                  Terminal Theme
                </h3>
                <p className="text-xs text-[#565f89] mb-3">
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
                      className="w-full h-12 rounded mb-2 border border-gray-700 flex items-center justify-center"
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
                      className="w-full h-12 rounded mb-2 border border-gray-700 flex items-center justify-center"
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
                      className="w-full h-12 rounded mb-2 border border-gray-300 flex items-center justify-center"
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
              <div className="p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                <h4 className="text-xs font-medium text-[#c0caf5] mb-2">
                  Theme Details
                </h4>
                <ul className="text-xs text-[#565f89] space-y-1">
                  <li><strong className="text-[#a9b1d6]">Terminal:</strong> Classic black & white</li>
                  <li><strong className="text-[#a9b1d6]">Tokyo Night:</strong> Dark blue with purple accents</li>
                  <li><strong className="text-[#a9b1d6]">Light:</strong> Bright mode for daytime use</li>
                </ul>
              </div>
            </>
          ) : activeTab === 'connectivity' ? (
            <>
              {/* Connection Method Selector */}
              <div className="mb-6">
                <h3 className="text-sm font-medium text-[#c0caf5] mb-3">
                  Connection Method
                </h3>
                <div className="grid grid-cols-2 gap-2">
                  <button
                    onClick={() => setConnectionMethod('local')}
                    className={`px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                      connectionMethod === 'local'
                        ? 'bg-[#7aa2f7] text-white'
                        : 'bg-[#1a1b26] text-[#565f89] hover:text-[#a9b1d6]'
                    }`}
                  >
                    Local
                  </button>
                  <button
                    onClick={() => setConnectionMethod('tailscale')}
                    className={`px-3 py-2 rounded-lg text-sm font-medium transition-colors ${
                      connectionMethod === 'tailscale'
                        ? 'bg-[#7dcfff] text-white'
                        : 'bg-[#1a1b26] text-[#565f89] hover:text-[#a9b1d6]'
                    }`}
                  >
                    Tailscale
                  </button>
                </div>
              </div>

              {/* Local Connection */}
              {connectionMethod === 'local' && (
                <>
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
                            <div className="w-16 h-16 rounded-full bg-yellow-500/20 flex items-center justify-center mb-4">
                              <svg className="w-8 h-8 text-yellow-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                              </svg>
                            </div>
                            <p className="text-sm text-yellow-400 text-center mb-2">
                              No local network IP detected
                            </p>
                            <p className="text-xs text-[#565f89] text-center mb-4">
                              Make sure your computer is connected to WiFi, or use Relay for remote connections.
                            </p>
                          </>
                        ) : (
                          <>
                            <p className="text-sm text-[#a9b1d6] mb-4 text-center">
                              Scan with MobileCLI to connect on your local network
                            </p>
                            <div className="bg-white p-4 rounded-lg mb-4">
                              <QRCodeCanvas
                                value={wsUrl}
                                size={180}
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

                  {/* Connection Info */}
                  <div className="space-y-3">
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
                        >
                          Copy
                        </button>
                      </div>
                    </div>
                    <div className="grid grid-cols-2 gap-4 text-sm">
                      <div>
                        <span className="text-[#565f89]">IP:</span>
                        <span className="ml-2 font-mono text-[#c0caf5]">{localIp || '...'}</span>
                      </div>
                      <div>
                        <span className="text-[#565f89]">Port:</span>
                        <span className="ml-2 font-mono text-[#c0caf5]">{wsPort}</span>
                      </div>
                    </div>
                  </div>
                </>
              )}


              {/* Tailscale Connection */}
              {connectionMethod === 'tailscale' && (
                <>
                  <div className="flex flex-col items-center mb-4">
                    <div className="w-16 h-16 rounded-full bg-[#7dcfff]/20 flex items-center justify-center mb-4">
                      <svg className="w-8 h-8 text-[#7dcfff]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7l9-4 9 4-9 4-9-4zm0 6l9 4 9-4m-9 4v6" />
                      </svg>
                    </div>
                    <p className="text-sm text-[#a9b1d6] text-center mb-1">
                      Connect over your Tailnet
                    </p>
                    <p className="text-xs text-[#565f89] text-center">
                      Private, encrypted tunnel via Tailscale
                    </p>
                  </div>

                  {tailscale?.running && tailscale.wsUrl ? (
                    <>
                      <div className="flex flex-col items-center mb-4">
                        <div className="bg-white p-4 rounded-lg mb-4">
                          <QRCodeCanvas
                            value={getTailscaleQrValue()}
                            size={180}
                            level="M"
                            bgColor="#ffffff"
                            fgColor="#1a1b26"
                          />
                        </div>
                      </div>

                      <div className="space-y-3">
                        <div>
                          <label className="text-xs font-medium text-[#565f89]">Tailscale URL</label>
                          <div className="mt-1 flex items-center gap-2">
                            <code className="flex-1 px-3 py-2 bg-[#1a1b26] rounded text-sm font-mono text-[#7dcfff] truncate">
                              {tailscale.wsUrl}
                            </code>
                            <button
                              onClick={() => navigator.clipboard.writeText(tailscale.wsUrl || '')}
                              className="px-3 py-2 bg-[#414868] hover:bg-[#565f89] rounded text-sm text-[#c0caf5]"
                            >
                              Copy
                            </button>
                          </div>
                        </div>
                        <div className="grid grid-cols-2 gap-4 text-sm">
                          <div>
                            <span className="text-[#565f89]">IP:</span>
                            <span className="ml-2 font-mono text-[#c0caf5]">{tailscale.tailscaleIp || '...'}</span>
                          </div>
                          <div>
                            <span className="text-[#565f89]">Host:</span>
                            <span className="ml-2 font-mono text-[#c0caf5]">{tailscale.hostname || '...'}</span>
                          </div>
                        </div>
                      </div>
                    </>
                  ) : (
                    <div className="p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                      <p className="text-sm text-[#a9b1d6] mb-3">
                        {tailscale?.installed
                          ? 'Tailscale is installed but not running.'
                          : 'Tailscale is not installed.'}
                      </p>
                      <ul className="text-xs text-[#565f89] space-y-1 list-disc list-inside mb-4">
                        <li>Install and sign in to Tailscale</li>
                        <li>Connect to your Tailnet</li>
                        <li>Return here to scan the QR code</li>
                      </ul>
                      <button
                        onClick={() => open('https://tailscale.com/download')}
                        className="w-full px-4 py-2 bg-[#7dcfff]/20 hover:bg-[#7dcfff]/30 text-[#7dcfff] rounded text-sm"
                      >
                        Install Tailscale
                      </button>
                    </div>
                  )}
                </>
              )}

              {/* Connection Tips */}
              <div className="mt-6 p-4 bg-[#1a1b26] rounded-lg border border-[#414868]/50">
                <h4 className="text-xs font-medium text-[#c0caf5] mb-2">
                  {connectionMethod === 'local' && 'Local Network Tips'}
                  {connectionMethod === 'tailscale' && 'Tailscale Benefits'}
                </h4>
                <ul className="text-xs text-[#565f89] space-y-1 list-disc list-inside">
                  {connectionMethod === 'local' && (
                    <>
                      <li>Both devices must be on the same WiFi</li>
                      <li>Fastest connection, no internet needed</li>
                      <li>Firewall may need to allow port {wsPort}</li>
                    </>
                  )}
                  {connectionMethod === 'tailscale' && (
                    <>
                      <li>Private encrypted tunnel</li>
                      <li>Works across networks</li>
                      <li>No port forwarding needed</li>
                    </>
                  )}
                </ul>
              </div>
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}

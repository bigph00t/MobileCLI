import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface HostSetupStepProps {
  onNext: () => void;
  onBack: () => void;
}

interface TailscaleStatus {
  installed: boolean;
  running: boolean;
  tailscaleIp: string | null;
  hostname: string | null;
  wsUrl: string | null;
}

export function HostSetupStep({ onNext, onBack }: HostSetupStepProps) {
  const [localIp, setLocalIp] = useState<string>('');
  const [tailscale, setTailscale] = useState<TailscaleStatus | null>(null);
  const [wsPort, setWsPort] = useState<number>(9847);
  const [showQr, setShowQr] = useState(false);

  useEffect(() => {
    // Get local IP
    invoke<string>('get_local_ip').then(setLocalIp).catch(console.error);

    // Get WebSocket port
    invoke<number>('get_ws_port').then(setWsPort).catch(console.error);

    // Get Tailscale status
    invoke<TailscaleStatus>('get_tailscale_status')
      .then(setTailscale)
      .catch(console.error);
  }, []);

  const localWsUrl = `ws://${localIp}:${wsPort}`;
  const tailscaleWsUrl = tailscale?.wsUrl || null;

  return (
    <div>
      <button
        onClick={onBack}
        className="text-gray-400 hover:text-white mb-4 flex items-center gap-1 text-sm"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
        </svg>
        Back
      </button>

      <h2 className="text-xl font-bold text-white mb-2">
        Host Setup
      </h2>

      <p className="text-gray-400 mb-6">
        Your desktop is ready to host sessions. Here's how to connect:
      </p>

      <div className="space-y-4">
        {/* Local Network */}
        <div className="bg-gray-700/50 rounded-lg p-4">
          <div className="flex items-center gap-2 mb-2">
            <div className="w-3 h-3 bg-green-500 rounded-full"></div>
            <span className="font-medium text-white">Local Network</span>
          </div>
          <p className="text-sm text-gray-400 mb-2">
            Connect from the same WiFi network
          </p>
          <code className="block bg-gray-900 px-3 py-2 rounded text-sm text-blue-400 font-mono">
            {localWsUrl}
          </code>
        </div>

        {/* Tailscale */}
        {tailscale?.running && tailscaleWsUrl && (
          <div className="bg-gray-700/50 rounded-lg p-4">
            <div className="flex items-center gap-2 mb-2">
              <div className="w-3 h-3 bg-purple-500 rounded-full"></div>
              <span className="font-medium text-white">Tailscale VPN</span>
            </div>
            <p className="text-sm text-gray-400 mb-2">
              Connect from anywhere via your Tailnet
            </p>
            <code className="block bg-gray-900 px-3 py-2 rounded text-sm text-purple-400 font-mono">
              {tailscaleWsUrl}
            </code>
          </div>
        )}

        {/* Relay */}
        <div className="bg-gray-700/50 rounded-lg p-4">
          <div className="flex items-center gap-2 mb-2">
            <div className="w-3 h-3 bg-blue-500 rounded-full"></div>
            <span className="font-medium text-white">Relay (E2E Encrypted)</span>
          </div>
          <p className="text-sm text-gray-400 mb-2">
            Connect from anywhere via secure relay server
          </p>
          <button
            onClick={() => setShowQr(true)}
            className="bg-blue-600 text-white px-4 py-2 rounded text-sm hover:bg-blue-700 transition-colors"
          >
            Generate QR Code
          </button>
        </div>
      </div>

      <div className="mt-6 pt-4 border-t border-gray-700">
        <div className="flex items-start gap-2 text-sm text-gray-400">
          <svg className="w-5 h-5 text-yellow-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span>
            You can generate a QR code anytime from the Settings panel to pair your mobile device.
          </span>
        </div>
      </div>

      <button
        onClick={onNext}
        className="w-full mt-6 bg-blue-600 text-white px-6 py-3 rounded-lg font-medium hover:bg-blue-700 transition-colors"
      >
        Continue
      </button>

      {/* QR Modal - simplified for now, will be enhanced later */}
      {showQr && (
        <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={() => setShowQr(false)}>
          <div className="bg-gray-800 rounded-lg p-6 max-w-sm w-full mx-4" onClick={e => e.stopPropagation()}>
            <h3 className="text-lg font-bold text-white mb-4">Connect Mobile App</h3>
            <p className="text-gray-400 text-sm mb-4">
              Open MobileCLI on your phone and scan this QR code to connect.
            </p>
            <div className="bg-white p-4 rounded-lg flex items-center justify-center">
              <span className="text-gray-500 text-sm">
                QR Code will appear here when relay is started
              </span>
            </div>
            <p className="text-xs text-gray-500 mt-4 text-center">
              Start the relay from Settings → Connection → Start Relay
            </p>
            <button
              onClick={() => setShowQr(false)}
              className="w-full mt-4 bg-gray-700 text-white px-4 py-2 rounded hover:bg-gray-600 transition-colors"
            >
              Close
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

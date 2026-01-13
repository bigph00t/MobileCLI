import { useState } from 'react';

interface ClientPairingStepProps {
  onNext: () => void;
  onBack: () => void;
}

export function ClientPairingStep({ onNext, onBack }: ClientPairingStepProps) {
  const [hostUrl, setHostUrl] = useState('');
  const [connectionMethod, setConnectionMethod] = useState<'qr' | 'manual'>('qr');
  const [error, setError] = useState<string | null>(null);

  const handleConnect = async () => {
    if (connectionMethod === 'manual' && !hostUrl.trim()) {
      setError('Please enter a host URL');
      return;
    }

    // For now, just proceed - actual connection will be implemented in Phase 7
    setError(null);
    onNext();
  };

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
        Connect to Host
      </h2>

      <p className="text-gray-400 mb-6">
        Connect to a computer running MobileCLI in Host mode.
      </p>

      {/* Connection method tabs */}
      <div className="flex gap-2 mb-6">
        <button
          onClick={() => setConnectionMethod('qr')}
          className={`flex-1 py-2 px-4 rounded-lg text-sm font-medium transition-colors ${
            connectionMethod === 'qr'
              ? 'bg-blue-600 text-white'
              : 'bg-gray-700 text-gray-400 hover:text-white'
          }`}
        >
          Scan QR Code
        </button>
        <button
          onClick={() => setConnectionMethod('manual')}
          className={`flex-1 py-2 px-4 rounded-lg text-sm font-medium transition-colors ${
            connectionMethod === 'manual'
              ? 'bg-blue-600 text-white'
              : 'bg-gray-700 text-gray-400 hover:text-white'
          }`}
        >
          Enter Manually
        </button>
      </div>

      {connectionMethod === 'qr' && (
        <div className="bg-gray-700/50 rounded-lg p-6">
          <div className="flex flex-col items-center">
            <div className="w-48 h-48 bg-gray-900 rounded-lg flex items-center justify-center mb-4">
              <div className="text-center">
                <svg className="w-12 h-12 text-gray-600 mx-auto mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 13a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
                <span className="text-gray-500 text-sm">Camera preview</span>
              </div>
            </div>
            <p className="text-sm text-gray-400 text-center">
              Point your camera at the QR code displayed on the host computer.
            </p>
            <p className="text-xs text-gray-500 mt-2">
              (Camera support coming soon - use manual entry for now)
            </p>
          </div>
        </div>
      )}

      {connectionMethod === 'manual' && (
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-gray-300 mb-2">
              Host URL
            </label>
            <input
              type="text"
              value={hostUrl}
              onChange={(e) => setHostUrl(e.target.value)}
              placeholder="ws://192.168.1.100:9847"
              className="w-full bg-gray-700 text-white px-4 py-3 rounded-lg border border-gray-600 focus:border-blue-500 focus:outline-none"
            />
            <p className="text-xs text-gray-500 mt-2">
              Enter the WebSocket URL from the host's Settings panel
            </p>
          </div>

          {error && (
            <div className="bg-red-900/50 text-red-400 px-4 py-2 rounded-lg text-sm">
              {error}
            </div>
          )}

          <div className="bg-gray-700/50 rounded-lg p-4">
            <h4 className="text-sm font-medium text-white mb-2">Where to find this:</h4>
            <ol className="text-sm text-gray-400 space-y-1 list-decimal list-inside">
              <li>Open MobileCLI on your host computer</li>
              <li>Go to Settings → Connection</li>
              <li>Copy the Local or Tailscale URL</li>
            </ol>
          </div>
        </div>
      )}

      <button
        onClick={handleConnect}
        className="w-full mt-6 bg-blue-600 text-white px-6 py-3 rounded-lg font-medium hover:bg-blue-700 transition-colors"
      >
        {connectionMethod === 'qr' ? 'Skip for Now' : 'Connect'}
      </button>
    </div>
  );
}

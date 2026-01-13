interface CompletionStepProps {
  mode: 'host' | 'client';
  onComplete: () => void;
  onBack: () => void;
}

export function CompletionStep({ mode, onComplete, onBack }: CompletionStepProps) {
  return (
    <div className="text-center">
      <button
        onClick={onBack}
        className="text-gray-400 hover:text-white mb-4 flex items-center gap-1 text-sm absolute left-8 top-8"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
        </svg>
        Back
      </button>

      {/* Success icon */}
      <div className="w-20 h-20 mx-auto mb-6 bg-green-600 rounded-full flex items-center justify-center">
        <svg className="w-10 h-10 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
        </svg>
      </div>

      <h2 className="text-xl font-bold text-white mb-2">
        You're All Set!
      </h2>

      <p className="text-gray-400 mb-6">
        {mode === 'host' ? (
          <>MobileCLI is configured as a <span className="text-green-400 font-medium">Host</span>. You can run Claude Code and Gemini CLI sessions and connect from your mobile device.</>
        ) : (
          <>MobileCLI is configured as a <span className="text-blue-400 font-medium">Client</span>. Connect to a host computer to view and control sessions remotely.</>
        )}
      </p>

      <div className="bg-gray-700/50 rounded-lg p-4 mb-6 text-left">
        <h4 className="text-sm font-medium text-white mb-3">Quick Tips:</h4>
        {mode === 'host' ? (
          <ul className="text-sm text-gray-400 space-y-2">
            <li className="flex items-start gap-2">
              <span className="text-green-400">•</span>
              <span>Click "New Session" to start a Claude Code or Gemini CLI session</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-green-400">•</span>
              <span>Go to Settings → Connection to get your pairing URL or QR code</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-green-400">•</span>
              <span>Your mobile app can connect via local network, Tailscale, or relay</span>
            </li>
          </ul>
        ) : (
          <ul className="text-sm text-gray-400 space-y-2">
            <li className="flex items-start gap-2">
              <span className="text-blue-400">•</span>
              <span>Scan a QR code or enter a host URL to connect</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-blue-400">•</span>
              <span>Sessions from the host will appear in your sidebar</span>
            </li>
            <li className="flex items-start gap-2">
              <span className="text-blue-400">•</span>
              <span>You can change to Host mode anytime in Settings</span>
            </li>
          </ul>
        )}
      </div>

      <button
        onClick={onComplete}
        className="w-full bg-blue-600 text-white px-6 py-3 rounded-lg font-medium hover:bg-blue-700 transition-colors"
      >
        Start Using MobileCLI
      </button>
    </div>
  );
}

interface ModeSelectionStepProps {
  onSelect: (mode: 'host' | 'client') => void;
  onBack: () => void;
}

export function ModeSelectionStep({ onSelect, onBack }: ModeSelectionStepProps) {
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
        Choose Your Mode
      </h2>

      <p className="text-gray-400 mb-6">
        How will you use this computer?
      </p>

      <div className="space-y-4">
        <button
          onClick={() => onSelect('host')}
          className="w-full p-4 border-2 border-gray-600 rounded-lg hover:border-blue-500 text-left transition-colors group"
        >
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 bg-green-600 rounded-lg flex items-center justify-center flex-shrink-0">
              <svg className="w-6 h-6 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
              </svg>
            </div>
            <div>
              <div className="text-lg font-semibold text-white group-hover:text-blue-400 transition-colors">
                Host Mode
              </div>
              <p className="text-sm text-gray-400 mt-1">
                This computer will run Claude Code / Gemini CLI sessions.
                Connect your phone or other devices to control sessions remotely.
              </p>
              <div className="flex items-center gap-2 mt-2 text-xs text-gray-500">
                <span className="bg-green-900/50 text-green-400 px-2 py-0.5 rounded">Recommended</span>
                <span>for your main development machine</span>
              </div>
            </div>
          </div>
        </button>

        <button
          onClick={() => onSelect('client')}
          className="w-full p-4 border-2 border-gray-600 rounded-lg hover:border-blue-500 text-left transition-colors group"
        >
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 bg-blue-600 rounded-lg flex items-center justify-center flex-shrink-0">
              <svg className="w-6 h-6 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
              </svg>
            </div>
            <div>
              <div className="text-lg font-semibold text-white group-hover:text-blue-400 transition-colors">
                Client Mode
              </div>
              <p className="text-sm text-gray-400 mt-1">
                Connect to another computer that's running sessions.
                View and control sessions remotely without running CLI locally.
              </p>
              <div className="flex items-center gap-2 mt-2 text-xs text-gray-500">
                <span>for secondary machines or remote access</span>
              </div>
            </div>
          </div>
        </button>
      </div>

      <p className="text-xs text-gray-500 mt-6 text-center">
        You can change this later in Settings
      </p>
    </div>
  );
}

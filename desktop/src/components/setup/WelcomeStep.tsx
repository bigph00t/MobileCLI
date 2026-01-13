interface WelcomeStepProps {
  onNext: () => void;
}

export function WelcomeStep({ onNext }: WelcomeStepProps) {
  return (
    <div className="text-center">
      {/* Logo */}
      <div className="w-24 h-24 mx-auto mb-6 bg-gradient-to-br from-blue-500 to-purple-600 rounded-2xl flex items-center justify-center">
        <svg
          className="w-14 h-14 text-white"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z"
          />
        </svg>
      </div>

      <h1 className="text-2xl font-bold text-white mb-2">
        Welcome to MobileCLI
      </h1>

      <p className="text-gray-400 mb-8 leading-relaxed">
        Control your Claude Code and Gemini CLI sessions from anywhere.
        Connect your phone or another computer for seamless remote access.
      </p>

      <button
        onClick={onNext}
        className="bg-blue-600 text-white px-8 py-3 rounded-lg font-medium hover:bg-blue-700 transition-colors"
      >
        Get Started
      </button>

      <p className="text-gray-500 text-sm mt-6">
        Setup takes less than a minute
      </p>
    </div>
  );
}

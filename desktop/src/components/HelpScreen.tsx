import { useState } from 'react';

interface HelpScreenProps {
  onClose: () => void;
}

interface HelpItemProps {
  question: string;
  answer: string;
}

function HelpItem({ question, answer }: HelpItemProps) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="border border-[#414868]/50 rounded-lg overflow-hidden">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="w-full flex items-center justify-between px-4 py-3 text-left hover:bg-[#1a1b26] transition-colors"
      >
        <span className="text-sm font-medium text-[#c0caf5]">{question}</span>
        <svg
          className={`w-4 h-4 text-[#565f89] transition-transform ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      {isOpen && (
        <div className="px-4 py-3 bg-[#1a1b26] border-t border-[#414868]/50">
          <p className="text-sm text-[#a9b1d6]">{answer}</p>
        </div>
      )}
    </div>
  );
}

export function HelpScreen({ onClose }: HelpScreenProps) {
  return (
    <div className="fixed inset-0 bg-[#1a1b26]/80 backdrop-blur-sm flex items-center justify-center z-50 p-4">
      <div className="bg-[#24283b] border border-[#414868] rounded-xl shadow-xl max-w-lg w-full max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#414868]">
          <h2 className="text-lg font-semibold text-[#c0caf5]">Help & FAQ</h2>
          <button
            onClick={onClose}
            className="p-1 text-[#565f89] hover:text-[#c0caf5] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Content - Scrollable */}
        <div className="p-6 overflow-y-auto flex-1">
          <div className="space-y-3">
            <HelpItem
              question="How do I connect my mobile app?"
              answer="Go to Settings (gear icon), then Connectivity tab. Choose a connection method: Local (same WiFi), Relay (anywhere, E2E encrypted), or Tailscale (private VPN). Scan the QR code with the MobileCLI mobile app."
            />

            <HelpItem
              question="What's the difference between Local and Relay?"
              answer="Local: Both devices must be on the same WiFi network. Fastest connection, lowest latency. Relay: Works from anywhere via our encrypted relay server (relay.mobilecli.app) - no VPN or port forwarding needed."
            />

            <HelpItem
              question="Why can't I connect?"
              answer="1. For Local: Make sure both devices are on the same WiFi network and your firewall allows port 9847. 2. For Relay: Check your internet connection. 3. Try generating a new QR code. 4. Restart both apps and try again."
            />

            <HelpItem
              question="Is my data encrypted?"
              answer="Yes! All Relay connections use end-to-end encryption (XSalsa20-Poly1305). The encryption key is embedded in the QR code and never sent to the relay server. Only your mobile device with the QR code can decrypt your messages."
            />

            <HelpItem
              question="Which CLIs are supported?"
              answer="MobileCLI supports Claude, Gemini, Codex, and OpenCode. Make sure they are installed on your computer. You can select your default CLI in Settings > General."
            />

            <HelpItem
              question="How do I approve tool calls?"
              answer="When your AI assistant requests to use a tool (like running a command), you'll see a notification on your mobile device. You can approve or reject it with a tap."
            />

            <HelpItem
              question="Can multiple devices connect?"
              answer="Yes! You can connect multiple devices (mobile + additional desktops). Go to Settings and use Relay mode. All connected devices will see the same sessions in real-time."
            />

            <HelpItem
              question="What if I lose connection?"
              answer="MobileCLI will automatically try to reconnect with exponential backoff. Your session data is preserved - just wait for it to reconnect or generate a new QR code."
            />
          </div>

          {/* Support link */}
          <div className="mt-6 pt-4 border-t border-[#414868]">
            <p className="text-sm text-[#565f89]">
              Need more help?{' '}
              <a
                href="https://github.com/mobilecli/desktop/issues"
                target="_blank"
                rel="noopener noreferrer"
                className="text-[#7aa2f7] hover:underline"
              >
                Open an issue on GitHub
              </a>
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}

export default HelpScreen;

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface AboutScreenProps {
  onClose: () => void;
}

export function AboutScreen({ onClose }: AboutScreenProps) {
  const [version, setVersion] = useState('');

  useEffect(() => {
    invoke<string>('get_version').then(setVersion);
  }, []);

  return (
    <div className="fixed inset-0 bg-[#1a1b26]/80 backdrop-blur-sm flex items-center justify-center z-50 p-4">
      <div className="bg-[#24283b] border border-[#414868] rounded-xl shadow-xl max-w-md w-full">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#414868]">
          <h2 className="text-lg font-semibold text-[#c0caf5]">About MobileCLI</h2>
          <button
            onClick={onClose}
            className="p-1 text-[#565f89] hover:text-[#c0caf5] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Content */}
        <div className="p-6">
          {/* Logo */}
          <div className="flex justify-center mb-4">
            <div className="w-20 h-20 rounded-2xl bg-gradient-to-br from-[#7aa2f7] to-[#bb9af7] flex items-center justify-center">
              <svg className="w-12 h-12 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
              </svg>
            </div>
          </div>

          <h1 className="text-xl font-bold text-[#c0caf5] text-center mb-1">
            MobileCLI Desktop
          </h1>

          <p className="text-[#bb9af7] text-center mb-4">
            Version {version || '...'}
          </p>

          <p className="text-[#a9b1d6] text-center text-sm mb-6">
            Control Claude Code and Gemini CLI sessions from anywhere. All connections are end-to-end encrypted.
          </p>

          {/* Links */}
          <div className="flex justify-center gap-6 text-sm">
            <a
              href="https://github.com/mobilecli/desktop"
              target="_blank"
              rel="noopener noreferrer"
              className="text-[#7aa2f7] hover:text-[#7dcfff] transition-colors flex items-center gap-1"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
              </svg>
              GitHub
            </a>
            <a
              href="https://mobilecli.app"
              target="_blank"
              rel="noopener noreferrer"
              className="text-[#7aa2f7] hover:text-[#7dcfff] transition-colors flex items-center gap-1"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
              </svg>
              Website
            </a>
            <a
              href="https://twitter.com/alexanderknigge"
              target="_blank"
              rel="noopener noreferrer"
              className="text-[#7aa2f7] hover:text-[#7dcfff] transition-colors flex items-center gap-1"
            >
              <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/>
              </svg>
              Twitter
            </a>
          </div>

          {/* Copyright */}
          <p className="text-[#565f89] text-xs text-center mt-6">
            &copy; 2026 MobileCLI. All rights reserved.
          </p>
        </div>
      </div>
    </div>
  );
}

export default AboutScreen;

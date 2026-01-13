import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';

interface TypingState {
  sessionId: string;
  typing: boolean;
  senderId: string;
}

interface TypingIndicatorProps {
  sessionId: string;
  currentSenderId?: string; // Don't show indicator for self
}

export function TypingIndicator({ sessionId, currentSenderId = 'local' }: TypingIndicatorProps) {
  const [typingState, setTypingState] = useState<TypingState | null>(null);

  useEffect(() => {
    const unlisten = listen<TypingState>('input-state', (event) => {
      if (event.payload.sessionId === sessionId) {
        // Only show indicator for other senders
        if (event.payload.senderId !== currentSenderId) {
          setTypingState(event.payload);
        }
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [sessionId, currentSenderId]);

  if (!typingState?.typing) {
    return null;
  }

  const senderName = getSenderName(typingState.senderId);

  return (
    <div className="flex items-center gap-2 text-[#565f89] text-sm py-2 px-4">
      <div className="flex gap-1">
        <span className="animate-bounce" style={{ animationDelay: '0ms' }}>.</span>
        <span className="animate-bounce" style={{ animationDelay: '150ms' }}>.</span>
        <span className="animate-bounce" style={{ animationDelay: '300ms' }}>.</span>
      </div>
      <span>{senderName} is typing...</span>
    </div>
  );
}

function getSenderName(senderId: string): string {
  if (senderId.startsWith('mobile-')) {
    return 'Mobile';
  }
  if (senderId.startsWith('desktop-')) {
    return 'Desktop client';
  }
  if (senderId === 'local') {
    return 'Local';
  }
  // Fallback - capitalize first letter
  return senderId.charAt(0).toUpperCase() + senderId.slice(1);
}

export default TypingIndicator;

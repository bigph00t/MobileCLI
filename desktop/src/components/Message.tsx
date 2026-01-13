import { useState } from 'react';
import { clsx } from 'clsx';
import { Message as MessageType } from '../hooks/useSession';

interface MessageProps {
  message: MessageType;
}

export default function Message({ message }: MessageProps) {
  const [isToolExpanded, setIsToolExpanded] = useState(false);

  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';
  const isTool = message.role === 'tool';
  const isSystem = message.role === 'system';

  if (isTool) {
    return (
      <div className="mx-4">
        <button
          onClick={() => setIsToolExpanded(!isToolExpanded)}
          className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 transition-colors"
        >
          <svg
            className={clsx(
              'w-4 h-4 transition-transform',
              isToolExpanded && 'rotate-90'
            )}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M9 5l7 7-7 7"
            />
          </svg>
          <span className="font-mono text-xs bg-gray-200 dark:bg-gray-700 px-2 py-0.5 rounded">
            {message.toolName || 'Tool'}
          </span>
          <span className="text-xs">
            {message.isStreaming ? 'Running...' : 'Completed'}
          </span>
        </button>
        {isToolExpanded && (
          <div className="mt-2 ml-6 p-3 bg-gray-100 dark:bg-gray-800 rounded-lg font-mono text-xs overflow-x-auto">
            <pre className="whitespace-pre-wrap break-words">
              {message.content}
            </pre>
            {message.toolResult && (
              <div className="mt-2 pt-2 border-t border-gray-200 dark:border-gray-700">
                <div className="text-gray-500 dark:text-gray-400 mb-1">
                  Result:
                </div>
                <pre className="whitespace-pre-wrap break-words">
                  {message.toolResult}
                </pre>
              </div>
            )}
          </div>
        )}
      </div>
    );
  }

  if (isSystem) {
    return (
      <div className="mx-4 text-center">
        <span className="text-xs text-gray-500 dark:text-gray-400 bg-gray-100 dark:bg-gray-800 px-3 py-1 rounded-full">
          {message.content}
        </span>
      </div>
    );
  }

  return (
    <div
      className={clsx(
        'flex',
        isUser ? 'justify-end' : 'justify-start'
      )}
    >
      <div
        className={clsx(
          'max-w-[80%] rounded-2xl px-4 py-3',
          isUser
            ? 'bg-primary-600 text-white rounded-br-md'
            : 'bg-white dark:bg-gray-800 text-gray-800 dark:text-gray-200 rounded-bl-md shadow-sm border border-gray-200 dark:border-gray-700'
        )}
      >
        {/* Role label for assistant */}
        {isAssistant && (
          <div className="text-xs text-gray-500 dark:text-gray-400 mb-1 font-medium">
            Claude
          </div>
        )}

        {/* Content */}
        <div
          className={clsx(
            'whitespace-pre-wrap break-words',
            isUser ? 'text-sm' : 'text-sm'
          )}
        >
          {message.content}
          {message.isStreaming && (
            <span className="inline-block w-2 h-4 ml-1 bg-current animate-pulse" />
          )}
        </div>

        {/* Timestamp */}
        <div
          className={clsx(
            'text-xs mt-2',
            isUser ? 'text-primary-200' : 'text-gray-400 dark:text-gray-500'
          )}
        >
          {formatTime(message.timestamp)}
        </div>
      </div>
    </div>
  );
}

function formatTime(timestamp: string): string {
  const date = new Date(timestamp);
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

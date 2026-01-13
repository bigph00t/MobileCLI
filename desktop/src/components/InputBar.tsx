import { useState, useRef, useEffect, KeyboardEvent, DragEvent } from 'react';

interface InputBarProps {
  onSend: (text: string) => void;
  disabled?: boolean;
  placeholder?: string;
}

export default function InputBar({
  onSend,
  disabled = false,
  placeholder = 'Type a message...',
}: InputBarProps) {
  const [text, setText] = useState('');
  const [isDragging, setIsDragging] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const dragCounterRef = useRef(0);

  // Auto-resize textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
    }
  }, [text]);

  const handleSend = () => {
    const trimmed = text.trim();
    if (trimmed && !disabled) {
      onSend(trimmed);
      setText('');
    }
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleDragEnter = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current++;
    if (e.dataTransfer.types.includes('Files')) {
      setIsDragging(true);
    }
  };

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounterRef.current--;
    if (dragCounterRef.current === 0) {
      setIsDragging(false);
    }
  };

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragging(false);
    dragCounterRef.current = 0;

    const files = Array.from(e.dataTransfer.files);
    if (files.length > 0) {
      // Get file paths and insert them
      const paths = files.map((file) => {
        // @ts-expect-error - Electron/Tauri provides path property
        const filePath = file.path || file.name;
        return filePath;
      });

      // Insert paths at cursor position or append
      const textarea = textareaRef.current;
      if (textarea) {
        const start = textarea.selectionStart;
        const end = textarea.selectionEnd;
        const pathText = paths.join('\n');
        const newText = text.slice(0, start) + pathText + text.slice(end);
        setText(newText);

        // Move cursor after inserted text
        setTimeout(() => {
          textarea.selectionStart = textarea.selectionEnd = start + pathText.length;
          textarea.focus();
        }, 0);
      } else {
        setText((prev) => prev + (prev ? '\n' : '') + paths.join('\n'));
      }
    }
  };

  return (
    <div
      className={`border-t border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4 transition-colors ${
        isDragging ? 'bg-primary-50 dark:bg-primary-900/20' : ''
      }`}
      onDragEnter={handleDragEnter}
      onDragLeave={handleDragLeave}
      onDragOver={handleDragOver}
      onDrop={handleDrop}
    >
      <div className="flex items-end gap-3 max-w-4xl mx-auto">
        <div className={`flex-1 relative ${isDragging ? 'pointer-events-none' : ''}`}>
          {isDragging && (
            <div className="absolute inset-0 flex items-center justify-center bg-primary-100 dark:bg-primary-900/40 rounded-xl border-2 border-dashed border-primary-500 z-10">
              <span className="text-primary-600 dark:text-primary-400 font-medium">
                Drop files here
              </span>
            </div>
          )}
          <textarea
            ref={textareaRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            disabled={disabled}
            rows={1}
            className="w-full resize-none rounded-xl border border-gray-300 dark:border-gray-600 bg-gray-50 dark:bg-gray-900 px-4 py-3 text-gray-800 dark:text-gray-200 placeholder-gray-500 focus:outline-none focus:ring-2 focus:ring-primary-500 focus:border-transparent disabled:opacity-50 disabled:cursor-not-allowed"
          />
        </div>
        <button
          onClick={handleSend}
          disabled={disabled || !text.trim()}
          className="flex-shrink-0 w-12 h-12 rounded-xl bg-primary-600 text-white flex items-center justify-center hover:bg-primary-700 active:bg-primary-800 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
        >
          <svg
            className="w-5 h-5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"
            />
          </svg>
        </button>
      </div>
      <div className="text-xs text-gray-400 dark:text-gray-500 mt-2 text-center">
        Press Enter to send, Shift+Enter for new line
      </div>
    </div>
  );
}

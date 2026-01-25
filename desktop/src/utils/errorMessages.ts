// User-friendly error messages for common error patterns

export const ERROR_MESSAGES: Record<string, string> = {
  // Connection errors
  'connection_refused': 'Cannot connect. Is the desktop app running?',
  'connection refused': 'Cannot connect. Check your firewall settings.',
  'network error': 'Network error. Check your internet connection.',
  'timeout': 'Operation timed out. Check your network connection.',
  'ETIMEDOUT': 'Connection timed out. Try again later.',
  'ECONNREFUSED': 'Connection refused. Is the server running?',
  'ENOTFOUND': 'Server not found. Check the URL.',

  // Relay errors
  'relay_unavailable': 'Relay server is not responding. Try again later.',
  'relay unavailable': 'Relay server is not responding. Try again later.',
  'all relay servers unavailable': 'All relay servers are offline. Please try again later.',
  'websocket connection failed': 'WebSocket connection failed. Check your network.',

  // Encryption errors
  'encryption_failed': 'Encryption error. Try reconnecting.',
  'decryption failed': 'Decryption error. The encryption key may have changed.',
  'invalid key': 'Invalid encryption key. Generate a new QR code.',

  // QR code errors
  'invalid_qr': 'Invalid QR code. Generate a new one from the desktop app.',
  'invalid qr': 'Invalid QR code format.',

  // Session errors
  'session_not_found': 'Session not found. It may have been closed.',
  'session not found': 'Session not found. It may have been closed.',
  'session closed': 'The session has been closed.',
  'no active session': 'No active session. Create a new one.',

  // PTY errors
  'pty_spawn_failed': 'Failed to start CLI. Check your installation.',
  'failed to spawn': 'Failed to start the CLI process.',
  'cli not found': 'CLI not found. Make sure it is installed.',
  'claude not installed': 'Claude CLI is not installed. Visit claude.ai/download to install it.',
  'gemini not installed': 'Gemini CLI is not installed. Visit google.com/gemini to install it.',

  // File errors
  'file not found': 'File not found.',
  'permission denied': 'Permission denied. Check file permissions.',
  'directory traversal': 'Invalid path. Access denied.',

  // Database errors
  'database error': 'Database error. Try restarting the app.',
  'db locked': 'Database is busy. Please wait.',
};

/**
 * Convert a raw error message to a user-friendly message
 */
export function getUserFriendlyError(error: string | Error): string {
  const errorStr = error instanceof Error ? error.message : error;
  const lowerError = errorStr.toLowerCase();

  // Check for exact matches first
  for (const [pattern, message] of Object.entries(ERROR_MESSAGES)) {
    if (lowerError.includes(pattern.toLowerCase())) {
      return message;
    }
  }

  // Generic fallback with cleaned error
  const cleanedError = errorStr
    .replace(/Error: /i, '')
    .replace(/\s+/g, ' ')
    .trim();

  return cleanedError || 'An unexpected error occurred.';
}

/**
 * Log error with context for debugging
 */
export function logError(context: string, error: unknown): void {
  if (import.meta.env.DEV) {
    console.error(`[${context}]`, error);
  }
}

/**
 * Check if an error is a network-related error
 */
export function isNetworkError(error: string | Error): boolean {
  const errorStr = error instanceof Error ? error.message : error;
  const networkPatterns = [
    'network',
    'connection',
    'timeout',
    'ETIMEDOUT',
    'ECONNREFUSED',
    'ENOTFOUND',
    'websocket',
    'socket',
  ];

  return networkPatterns.some(pattern =>
    errorStr.toLowerCase().includes(pattern.toLowerCase())
  );
}

/**
 * Check if an error is recoverable (can retry)
 */
export function isRecoverableError(error: string | Error): boolean {
  const errorStr = error instanceof Error ? error.message : error;
  const recoverablePatterns = [
    'timeout',
    'network',
    'unavailable',
    'busy',
    'locked',
    'try again',
  ];

  return recoverablePatterns.some(pattern =>
    errorStr.toLowerCase().includes(pattern.toLowerCase())
  );
}

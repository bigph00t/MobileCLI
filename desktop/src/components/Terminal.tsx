import { useEffect, useRef, useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { Terminal as XTerm, ITheme } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

// Terminal theme presets
export type TerminalThemeName = 'classic' | 'tokyo-night' | 'light';

export const TERMINAL_THEMES: Record<TerminalThemeName, ITheme> = {
  // Classic black/white terminal (default)
  classic: {
    background: '#000000',
    foreground: '#ffffff',
    cursor: '#ffffff',
    cursorAccent: '#000000',
    selectionBackground: '#444444',
    black: '#000000',
    red: '#ff5555',
    green: '#55ff55',
    yellow: '#ffff55',
    blue: '#5555ff',
    magenta: '#ff55ff',
    cyan: '#55ffff',
    white: '#ffffff',
    brightBlack: '#555555',
    brightRed: '#ff5555',
    brightGreen: '#55ff55',
    brightYellow: '#ffff55',
    brightBlue: '#5555ff',
    brightMagenta: '#ff55ff',
    brightCyan: '#55ffff',
    brightWhite: '#ffffff',
  },
  // Tokyo Night theme (blue/purple)
  'tokyo-night': {
    background: '#1a1b26',
    foreground: '#a9b1d6',
    cursor: '#c0caf5',
    cursorAccent: '#1a1b26',
    selectionBackground: '#33467c',
    black: '#15161e',
    red: '#f7768e',
    green: '#9ece6a',
    yellow: '#e0af68',
    blue: '#7aa2f7',
    magenta: '#bb9af7',
    cyan: '#7dcfff',
    white: '#a9b1d6',
    brightBlack: '#414868',
    brightRed: '#f7768e',
    brightGreen: '#9ece6a',
    brightYellow: '#e0af68',
    brightBlue: '#7aa2f7',
    brightMagenta: '#bb9af7',
    brightCyan: '#7dcfff',
    brightWhite: '#c0caf5',
  },
  // Light mode
  light: {
    background: '#ffffff',
    foreground: '#1f2937',
    cursor: '#1f2937',
    cursorAccent: '#ffffff',
    selectionBackground: '#bfdbfe',
    black: '#1f2937',
    red: '#dc2626',
    green: '#16a34a',
    yellow: '#ca8a04',
    blue: '#2563eb',
    magenta: '#9333ea',
    cyan: '#0891b2',
    white: '#f3f4f6',
    brightBlack: '#6b7280',
    brightRed: '#ef4444',
    brightGreen: '#22c55e',
    brightYellow: '#eab308',
    brightBlue: '#3b82f6',
    brightMagenta: '#a855f7',
    brightCyan: '#06b6d4',
    brightWhite: '#ffffff',
  },
};

// Get current theme from localStorage (default: classic)
export function getCurrentTerminalTheme(): TerminalThemeName {
  const saved = localStorage.getItem('terminalTheme');
  if (saved && saved in TERMINAL_THEMES) {
    return saved as TerminalThemeName;
  }
  return 'classic';
}

// Save theme to localStorage
export function setTerminalTheme(theme: TerminalThemeName) {
  localStorage.setItem('terminalTheme', theme);
  // Dispatch custom event so terminals can update
  window.dispatchEvent(new CustomEvent('terminal-theme-change', { detail: theme }));
}

interface TerminalProps {
  sessionId: string;
  onData?: (data: string) => void;
}

// Store terminal instances with their container elements
interface TerminalInstance {
  term: XTerm;
  fitAddon: FitAddon;
  container: HTMLDivElement;
  buffer: string[];
  initialized: boolean;
  inputBuffer: string; // Track current input line (not yet submitted)
  cursorPosition: number; // Track cursor position within input buffer
  onDataCallback?: (data: string) => void; // Callback for sending input - updated each mount
}

const terminals = new Map<string, TerminalInstance>();

// Generate unique sender ID for this desktop instance (for echo prevention in multi-device sync)
const DESKTOP_SENDER_ID = `desktop-${crypto.randomUUID().slice(0, 8)}`;

// Emit input state to sync with mobile clients
async function emitInputState(sessionId: string, text: string, cursorPosition: number) {
  try {
    await emit('input-state', {
      sessionId,
      text,
      cursorPosition,
      senderId: DESKTOP_SENDER_ID,
      timestamp: Date.now(),
    });
  } catch (e) {
    console.error('Failed to emit input state:', e);
  }
}

// Write to a specific session's terminal
export function writeToTerminal(sessionId: string, data: string) {
  const instance = terminals.get(sessionId);
  if (instance?.initialized && instance.term) {
    instance.term.write(data);
    // Ensure terminal has focus after receiving output
    // This helps recover from focus-loss scenarios
    if (instance.container.style.display !== 'none') {
      instance.term.focus();
    }
  } else if (instance) {
    instance.buffer.push(data);
  } else {
    // Create placeholder with buffer for when terminal mounts
    terminals.set(sessionId, {
      term: null as any,
      fitAddon: null as any,
      container: null as any,
      buffer: [data],
      initialized: false,
      inputBuffer: '',
      cursorPosition: 0,
    });
  }
}

// Clean up a terminal instance completely
export function disposeTerminal(sessionId: string) {
  const instance = terminals.get(sessionId);
  if (instance) {
    if (instance.term) {
      instance.term.dispose();
    }
    if (instance.container?.parentNode) {
      instance.container.parentNode.removeChild(instance.container);
    }
    terminals.delete(sessionId);
  }
}

export default function Terminal({ sessionId, onData }: TerminalProps) {
  const wrapperRef = useRef<HTMLDivElement>(null);
  const onDataRef = useRef(onData);
  const [themeName, setThemeName] = useState<TerminalThemeName>(getCurrentTerminalTheme);

  // Listen for theme changes and update all terminals
  useEffect(() => {
    const handleThemeChange = (event: CustomEvent<TerminalThemeName>) => {
      const newTheme = event.detail;
      setThemeName(newTheme);
      const theme = TERMINAL_THEMES[newTheme];

      // Update all terminal instances
      terminals.forEach((instance) => {
        if (instance.term) {
          instance.term.options.theme = theme;
        }
        if (instance.container) {
          instance.container.style.backgroundColor = theme.background || '#000000';
        }
      });
    };

    window.addEventListener('terminal-theme-change', handleThemeChange as EventListener);
    return () => {
      window.removeEventListener('terminal-theme-change', handleThemeChange as EventListener);
    };
  }, []);

  // Keep onData ref updated AND sync to terminal instance
  useEffect(() => {
    console.log('[Terminal] First useEffect running:', {
      sessionId,
      hasOnData: !!onData,
      hasInstance: !!terminals.get(sessionId),
      instanceInitialized: terminals.get(sessionId)?.initialized,
    });
    onDataRef.current = onData;
    // Also update the callback on the terminal instance if it exists
    const instance = terminals.get(sessionId);
    if (instance) {
      instance.onDataCallback = onData;
      console.log('[Terminal] Updated onDataCallback on existing instance');
      // FIX: Ensure terminal is focused when callback updates
      // This helps with sync issues where terminal appears unresponsive
      if (instance.initialized && instance.term) {
        instance.term.focus();
      }
    } else {
      console.log('[Terminal] No instance found in first useEffect');
    }
  }, [onData, sessionId]);

  const sendResize = useCallback(async (sid: string, rows: number, cols: number) => {
    try {
      await invoke('resize_pty', { sessionId: sid, rows, cols });
    } catch (e) {
      // Ignore resize errors for non-active sessions
    }
  }, []);

  useEffect(() => {
    console.log('[Terminal] Second useEffect running:', {
      sessionId,
      hasWrapper: !!wrapperRef.current,
      hasOnDataRef: !!onDataRef.current,
    });
    if (!wrapperRef.current) return;

    const wrapper = wrapperRef.current;

    // Hide all terminal containers
    terminals.forEach((instance, sid) => {
      if (instance.container) {
        instance.container.style.display = sid === sessionId ? 'block' : 'none';
      }
    });

    // Get or create terminal for this session
    let instance = terminals.get(sessionId);
    console.log('[Terminal] Instance check:', {
      exists: !!instance,
      initialized: instance?.initialized,
      hasCallback: !!instance?.onDataCallback,
    });

    if (!instance || !instance.initialized) {
      console.log('[Terminal] Creating new terminal for session:', sessionId);
      // Get any buffered data from JS memory
      const bufferedData = instance?.buffer || [];

      // Create container for this terminal
      const container = document.createElement('div');
      container.style.width = '100%';
      container.style.height = '100%';
      container.style.display = 'block';
      wrapper.appendChild(container);

      // Get current theme
      const currentTheme = getCurrentTerminalTheme();
      const theme = TERMINAL_THEMES[currentTheme];

      const term = new XTerm({
        cursorBlink: true,
        fontSize: 13,
        fontFamily: 'JetBrains Mono, Menlo, Monaco, "Courier New", monospace',
        theme,
        scrollback: 10000,
        allowProposedApi: true,
      });

      // Set container background to match theme
      container.style.backgroundColor = theme.background || '#000000';

      const fitAddon = new FitAddon();
      term.loadAddon(fitAddon);

      instance = { term, fitAddon, container, buffer: [], initialized: true, inputBuffer: '', cursorPosition: 0, onDataCallback: onDataRef.current };
      terminals.set(sessionId, instance);
      console.log('[Terminal] Created new terminal instance:', {
        sessionId,
        hasOnDataCallback: !!instance.onDataCallback,
        onDataRefCurrent: !!onDataRef.current,
      });

      // Handle user input - filter out focus events and track input state
      term.onData((data) => {
        console.log('[Terminal] onData called:', {
          data: data.length > 20 ? data.slice(0, 20) + '...' : data,
          hex: Array.from(data).map(c => c.charCodeAt(0).toString(16)).join(' '),
          sessionId,
          hasOnDataRef: !!onDataRef.current,
        });

        if (data === '\x1b[I' || data === '\x1b[O') {
          console.log('[Terminal] Filtered focus event');
          return;
        }

        // Track input buffer for mobile sync
        const inst = terminals.get(sessionId);
        if (inst) {
          // Handle different input types
          if (data === '\r' || data === '\n') {
            // Enter pressed - clear input buffer
            inst.inputBuffer = '';
            inst.cursorPosition = 0;
            emitInputState(sessionId, '', 0);
          } else if (data === '\x7f' || data === '\b') {
            // Backspace - remove character before cursor
            if (inst.cursorPosition > 0) {
              inst.inputBuffer =
                inst.inputBuffer.slice(0, inst.cursorPosition - 1) +
                inst.inputBuffer.slice(inst.cursorPosition);
              inst.cursorPosition--;
              emitInputState(sessionId, inst.inputBuffer, inst.cursorPosition);
            }
          } else if (data === '\x1b[D') {
            // Left arrow - move cursor left
            if (inst.cursorPosition > 0) {
              inst.cursorPosition--;
              emitInputState(sessionId, inst.inputBuffer, inst.cursorPosition);
            }
          } else if (data === '\x1b[C') {
            // Right arrow - move cursor right
            if (inst.cursorPosition < inst.inputBuffer.length) {
              inst.cursorPosition++;
              emitInputState(sessionId, inst.inputBuffer, inst.cursorPosition);
            }
          } else if (data === '\x03') {
            // Ctrl+C - clear input
            inst.inputBuffer = '';
            inst.cursorPosition = 0;
            emitInputState(sessionId, '', 0);
          } else if (data === '\x15') {
            // Ctrl+U - clear line
            inst.inputBuffer = '';
            inst.cursorPosition = 0;
            emitInputState(sessionId, '', 0);
          } else if (data.length === 1 && data.charCodeAt(0) >= 32) {
            // Regular printable character - insert at cursor position
            inst.inputBuffer =
              inst.inputBuffer.slice(0, inst.cursorPosition) +
              data +
              inst.inputBuffer.slice(inst.cursorPosition);
            inst.cursorPosition++;
            emitInputState(sessionId, inst.inputBuffer, inst.cursorPosition);
          } else if (data.length > 1 && !data.startsWith('\x1b')) {
            // Pasted text - insert at cursor position
            inst.inputBuffer =
              inst.inputBuffer.slice(0, inst.cursorPosition) +
              data +
              inst.inputBuffer.slice(inst.cursorPosition);
            inst.cursorPosition += data.length;
            emitInputState(sessionId, inst.inputBuffer, inst.cursorPosition);
          }
        }

        // Use the callback stored in the instance (updated each mount to avoid stale refs)
        const currentInst = terminals.get(sessionId);
        console.log('[Terminal] Calling onDataCallback, exists:', !!currentInst?.onDataCallback);
        currentInst?.onDataCallback?.(data);
        console.log('[Terminal] onDataCallback called successfully');
      });

      // Open terminal in its container
      term.open(container);

      // Small delay to ensure DOM is ready
      requestAnimationFrame(() => {
        fitAddon.fit();
        sendResize(sessionId, term.rows, term.cols);

        // Write JS-buffered data
        bufferedData.forEach((data) => term.write(data));

        // Focus the terminal so user can type immediately
        term.focus();
      });
    } else {
      // Terminal exists - make sure its container is in the wrapper and visible
      if (!wrapper.contains(instance.container)) {
        wrapper.appendChild(instance.container);
      }
      instance.container.style.display = 'block';

      // CRITICAL: Update the onDataCallback to the current component's callback
      // This fixes the stale ref issue when the component remounts
      instance.onDataCallback = onDataRef.current;
      console.log('[Terminal] Updated onDataCallback for existing terminal, sessionId:', sessionId);

      // Re-fit and send resize
      requestAnimationFrame(() => {
        if (instance) {
          instance.fitAddon.fit();
          sendResize(sessionId, instance.term.rows, instance.term.cols);
          instance.term.focus();
        }
      });
    }

    // Handle resize
    const handleResize = () => {
      const inst = terminals.get(sessionId);
      if (inst?.fitAddon && inst?.term && inst.container.style.display !== 'none') {
        try {
          inst.fitAddon.fit();
          sendResize(sessionId, inst.term.rows, inst.term.cols);
        } catch (e) {
          // Ignore
        }
      }
    };

    const resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(wrapper);
    window.addEventListener('resize', handleResize);

    // FIX: Focus terminal when window gains focus
    // This ensures the terminal is responsive after user switches windows
    const handleWindowFocus = () => {
      const inst = terminals.get(sessionId);
      if (inst?.initialized && inst?.term && inst.container.style.display !== 'none') {
        inst.term.focus();
      }
    };
    window.addEventListener('focus', handleWindowFocus);

    // Also handle visibility change (tab switching, etc.)
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        const inst = terminals.get(sessionId);
        if (inst?.initialized && inst?.term && inst.container.style.display !== 'none') {
          // Small delay to ensure window is properly focused
          setTimeout(() => inst.term.focus(), 50);
        }
      }
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);

    // FIX: Global keydown handler to ensure terminal focus when typing
    // This captures the case where focus is lost to an unknown element
    const handleGlobalKeyDown = (e: KeyboardEvent) => {
      // Don't steal focus from input elements
      const activeElement = document.activeElement;
      const isInputFocused = activeElement instanceof HTMLInputElement ||
                            activeElement instanceof HTMLTextAreaElement ||
                            activeElement?.getAttribute('contenteditable') === 'true';
      if (isInputFocused) return;

      // Ignore modifier-only keys
      if (e.key === 'Shift' || e.key === 'Control' || e.key === 'Alt' || e.key === 'Meta') return;

      // If this terminal is visible, focus it and forward the key
      const inst = terminals.get(sessionId);
      if (inst?.initialized && inst?.term && inst.container.style.display !== 'none') {
        const termElement = inst.term.element;
        if (termElement && !termElement.contains(activeElement)) {
          console.log('[Terminal] Global keydown detected, focusing terminal for session:', sessionId, 'key:', e.key);
          inst.term.focus();

          // Forward the keypress to xterm via onDataCallback
          // This ensures the first keypress isn't lost when focus was elsewhere
          if (inst.onDataCallback) {
            let data = '';
            if (e.key === 'Enter') {
              data = '\r';
            } else if (e.key === 'Backspace') {
              data = '\x7f';
            } else if (e.key === 'Escape') {
              data = '\x1b';
            } else if (e.key === 'Tab') {
              data = '\t';
            } else if (e.key === 'ArrowUp') {
              data = '\x1b[A';
            } else if (e.key === 'ArrowDown') {
              data = '\x1b[B';
            } else if (e.key === 'ArrowRight') {
              data = '\x1b[C';
            } else if (e.key === 'ArrowLeft') {
              data = '\x1b[D';
            } else if (e.ctrlKey && e.key.length === 1) {
              // Ctrl+letter (like Ctrl+C = \x03)
              const code = e.key.toLowerCase().charCodeAt(0) - 96;
              if (code > 0 && code < 27) {
                data = String.fromCharCode(code);
              }
            } else if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
              // Regular printable character
              data = e.key;
            }

            if (data) {
              console.log('[Terminal] Forwarding captured keypress to PTY:', JSON.stringify(data));
              inst.onDataCallback(data);
              e.preventDefault(); // Prevent double input
            }
          }
        }
      }
    };
    // Use capture phase to intercept before other handlers
    document.addEventListener('keydown', handleGlobalKeyDown, true);

    // Handle theme changes
    const handleThemeChange = (e: CustomEvent<TerminalThemeName>) => {
      const newTheme = TERMINAL_THEMES[e.detail];
      if (!newTheme) return;

      // Update all terminal instances with the new theme
      terminals.forEach((inst) => {
        if (inst.initialized && inst.term) {
          inst.term.options.theme = newTheme;
          // Update container background too
          if (inst.container) {
            inst.container.style.backgroundColor = newTheme.background || '#000000';
          }
        }
      });
    };
    window.addEventListener('terminal-theme-change', handleThemeChange as EventListener);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener('resize', handleResize);
      window.removeEventListener('focus', handleWindowFocus);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      document.removeEventListener('keydown', handleGlobalKeyDown, true);
      window.removeEventListener('terminal-theme-change', handleThemeChange as EventListener);

      // Hide this terminal's container when unmounting (but don't remove it)
      const inst = terminals.get(sessionId);
      if (inst?.container) {
        inst.container.style.display = 'none';
      }
    };
  }, [sessionId, sendResize]);

  // Listen for input state from mobile clients via Tauri events
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupListener = async () => {
      unlisten = await listen<{ sessionId: string; text?: string; cursorPosition?: number; typing?: boolean; senderId?: string }>(
        'input-state',
        (event) => {
          // Only handle events for this session
          if (event.payload.sessionId !== sessionId) return;

          // CRITICAL: Distinguish between input sync (has 'text') and typing indicator (has 'typing')
          // The same event name is used for both - typing indicators from lib.rs don't have 'text'
          if (typeof event.payload.text !== 'string') {
            // This is a typing indicator event, not input sync - ignore it here
            // Typing indicators are handled by TypingIndicator component
            return;
          }

          // Simple echo prevention - ignore our own messages
          if (event.payload.senderId === DESKTOP_SENDER_ID) {
            return;
          }

          const instance = terminals.get(sessionId);
          if (!instance?.initialized) return;

          const newText = event.payload.text;

          // Update local tracking
          instance.inputBuffer = newText;
          instance.cursorPosition = event.payload.cursorPosition ?? newText.length;

          console.log('[Terminal] Remote input from:', event.payload.senderId, ':', newText.slice(0, 50));
        }
      );
    };

    setupListener();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [sessionId]);

  // Listen for input state requests from mobile clients (when they subscribe to a session)
  // This ensures mobile sees any pending input the desktop user has typed
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupListener = async () => {
      unlisten = await listen<{ sessionId: string }>(
        'request-input-state',
        (event) => {
          // Only handle events for this session
          if (event.payload.sessionId !== sessionId) return;

          const instance = terminals.get(sessionId);
          if (!instance?.initialized) return;

          console.log('[Terminal] Received request-input-state, sending current inputBuffer:', instance.inputBuffer);

          // Emit current input state so mobile can sync
          emitInputState(sessionId, instance.inputBuffer, instance.cursorPosition);
        }
      );
    };

    setupListener();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [sessionId]);

  // Click handler to ensure terminal focus
  const handleClick = useCallback(() => {
    const instance = terminals.get(sessionId);
    if (instance?.initialized && instance.term) {
      instance.term.focus();
    }
  }, [sessionId]);

  // FIX: Capture keyboard events at wrapper level and ensure terminal has focus
  // This fixes the issue where the terminal loses focus and user can't type
  const handleKeyDown = useCallback((_e: React.KeyboardEvent) => {
    const instance = terminals.get(sessionId);
    if (instance?.initialized && instance.term) {
      // If terminal doesn't have focus, focus it and let xterm handle the key
      const termElement = instance.term.element;
      if (termElement && !termElement.contains(document.activeElement)) {
        console.log('[Terminal] Wrapper captured keydown, focusing terminal');
        instance.term.focus();
        // Don't prevent default - let the key event flow naturally
      }
    }
  }, [sessionId]);

  // FIX: Focus terminal on mouse enter (user is intending to interact)
  const handleMouseEnter = useCallback(() => {
    const instance = terminals.get(sessionId);
    if (instance?.initialized && instance.term && instance.container.style.display !== 'none') {
      // Only focus if no other input element is focused
      const activeElement = document.activeElement;
      const isInputFocused = activeElement instanceof HTMLInputElement ||
                            activeElement instanceof HTMLTextAreaElement;
      if (!isInputFocused) {
        instance.term.focus();
      }
    }
  }, [sessionId]);

  return (
    <div
      ref={wrapperRef}
      className="w-full h-full terminal-wrapper"
      style={{ backgroundColor: TERMINAL_THEMES[themeName].background, padding: '0' }}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      onMouseEnter={handleMouseEnter}
      tabIndex={0}
    />
  );
}

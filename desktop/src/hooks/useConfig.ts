import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export type AppMode = 'host' | 'client';

export interface AppConfig {
  mode: AppMode;
  version: string;
  firstRun: boolean;
  relayUrls: string[];
  lastHostUrl: string | null;
  lastRoomCode: string | null;
  wsPort: number;
}

interface ConfigState {
  config: AppConfig | null;
  isLoading: boolean;
  error: string | null;

  // Actions
  fetchConfig: () => Promise<void>;
  setConfig: (config: AppConfig) => Promise<void>;
  setAppMode: (mode: AppMode) => Promise<void>;
  setFirstRunComplete: () => Promise<void>;
  isFirstRun: () => Promise<boolean>;
}

const defaultConfig: AppConfig = {
  mode: 'host',
  version: '0.1.0',
  firstRun: true,
  relayUrls: ['wss://relay.mobilecli.app'],
  lastHostUrl: null,
  lastRoomCode: null,
  wsPort: 9847,
};

export const useConfigStore = create<ConfigState>((set, get) => ({
  config: null,
  isLoading: false,
  error: null,

  fetchConfig: async () => {
    set({ isLoading: true, error: null });
    try {
      const config = await invoke<AppConfig>('get_config');
      // Convert snake_case from Rust to camelCase for TypeScript
      const normalizedConfig: AppConfig = {
        mode: config.mode,
        version: config.version,
        firstRun: (config as any).first_run ?? config.firstRun,
        relayUrls: (config as any).relay_urls ?? config.relayUrls ?? defaultConfig.relayUrls,
        lastHostUrl: (config as any).last_host_url ?? config.lastHostUrl,
        lastRoomCode: (config as any).last_room_code ?? config.lastRoomCode,
        wsPort: (config as any).ws_port ?? config.wsPort ?? defaultConfig.wsPort,
      };
      set({ config: normalizedConfig, isLoading: false });
    } catch (e) {
      console.error('Failed to fetch config:', e);
      // Use default config on error
      set({ config: defaultConfig, error: String(e), isLoading: false });
    }
  },

  setConfig: async (config: AppConfig) => {
    set({ isLoading: true, error: null });
    try {
      // Convert camelCase to snake_case for Rust
      const rustConfig = {
        mode: config.mode,
        version: config.version,
        first_run: config.firstRun,
        relay_urls: config.relayUrls,
        last_host_url: config.lastHostUrl,
        last_room_code: config.lastRoomCode,
        ws_port: config.wsPort,
      };
      await invoke('set_config', { config: rustConfig });
      set({ config, isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
      throw e;
    }
  },

  setAppMode: async (mode: AppMode) => {
    const currentConfig = get().config;
    if (!currentConfig) {
      // Fetch config first if not loaded
      await get().fetchConfig();
    }

    try {
      await invoke('set_app_mode', { mode });
      set((state) => ({
        config: state.config ? { ...state.config, mode } : null,
      }));
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  setFirstRunComplete: async () => {
    try {
      await invoke('set_first_run_complete');
      set((state) => ({
        config: state.config ? { ...state.config, firstRun: false } : null,
      }));
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },

  isFirstRun: async () => {
    try {
      return await invoke<boolean>('is_first_run');
    } catch (e) {
      console.error('Failed to check first run:', e);
      return true; // Assume first run on error
    }
  },
}));

// Convenience hook for common operations
export function useConfig() {
  const store = useConfigStore();
  return {
    config: store.config,
    isLoading: store.isLoading,
    error: store.error,
    fetchConfig: store.fetchConfig,
    setConfig: store.setConfig,
    setAppMode: store.setAppMode,
    setFirstRunComplete: store.setFirstRunComplete,
    isFirstRun: store.isFirstRun,
    isHostMode: store.config?.mode === 'host',
    isClientMode: store.config?.mode === 'client',
  };
}

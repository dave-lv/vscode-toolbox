// Typed wrappers for the Rust commands (app/src-tauri/src/commands.rs).
import { invoke as tauriInvoke, isTauri } from "@tauri-apps/api/core";
import { mockInvoke } from "./mock";
import type { Agent, Settings, UiState } from "./types";

// In a plain browser (`npm run dev` without Tauri) the fixture in mock.ts answers instead.
const invoke = isTauri() ? tauriInvoke : mockInvoke;

export const ipc = {
  getState: () => invoke<UiState>("get_state"),
  openProject: (uri: string, opts: { profileId?: string | null; newWindow?: boolean; skipLayout?: boolean } = {}) =>
    invoke<void>("open_project", {
      uri,
      profileId: opts.profileId ?? null,
      newWindow: opts.newWindow ?? false,
      skipLayout: opts.skipLayout ?? false,
    }),
  newWindow: (profileId: string) => invoke<void>("new_window", { profileId }),
  openFile: (path: string, profileId: string | null = null) => invoke<void>("open_file", { path, profileId }),
  openConfig: () => invoke<void>("open_config"),
  installDevLayout: (profileId: string) => invoke<void>("install_dev_layout", { profileId }),
  closeWindow: (uri: string) => invoke<void>("close_window", { uri }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  toggleFavorite: (uri: string) => invoke<void>("toggle_favorite", { uri }),
  moveFavorite: (uri: string, toIndex: number) => invoke<void>("move_favorite", { uri, toIndex }),
  hide: (uri: string) => invoke<void>("hide", { uri }),
  unhide: (uri: string) => invoke<void>("unhide", { uri }),
  setAgent: (uri: string, agent: Agent | null) => invoke<void>("set_agent", { uri, agent }),
  rename: (uri: string, name: string | null) => invoke<void>("rename", { uri, name }),
  updateSettings: (patch: Partial<Omit<Settings, "scanDepth">>) => invoke<void>("update_settings", { patch }),
  resetConfig: () => invoke<void>("reset_config"),
  checkUpdates: () => invoke<void>("check_updates"),
  installUpdate: () => invoke<void>("install_update"),
  dismissUpdate: () => invoke<void>("dismiss_update"),
  importToolboxFavorites: () => invoke<void>("import_toolbox_favorites"),
  setLaunchAtLogin: (on: boolean) => invoke<void>("set_launch_at_login", { on }),
  dismissBanner: () => invoke<void>("dismiss_banner"),
  hidePanel: () => invoke<void>("hide_panel"),
  quit: () => invoke<void>("quit"),
  panelPainted: () => invoke<number | null>("panel_painted"),
};

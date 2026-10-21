// Mirrors app/src-tauri/src/ui.rs (UiState and friends).

export type Os = "macos" | "windows" | "linux";

export type ProjectKind =
  | "folder"
  | "workspaceFile"
  | "attachedContainer"
  | "devContainer"
  | "ssh"
  | "wsl"
  | "remote";

export interface GitInfo {
  branch: string;
  dirty: boolean;
}

export interface Row {
  uri: string;
  /** Alias from config.json, else the project's own name. */
  name: string;
  baseName: string;
  detail: string;
  tag: string | null;
  kind: ProjectKind;
  isWorkspace: boolean;
  /** ms since the epoch */
  lastOpened: number | null;
  profileId: string | null;
  hostPath: string | null;
  /** What "Copy Path" copies: the local folder, or the decoded path on the remote. */
  path: string;
  open: boolean;
  favorite: boolean;
  git: GitInfo | null;
  /** The dev-layout agent chosen for this project; null: the profile's setting. */
  agent: Agent | null;
}

export type Agent = "claude" | "codex";

export interface ExtensionInfo {
  id: string;
  version: string;
  displayName: string | null;
}

export interface ProfileInfo {
  id: string;
  name: string;
  icon: string | null;
  isBuiltin: boolean;
  isDefault: boolean;
  usesDefaultExtensions: boolean;
  extensions: ExtensionInfo[];
  projectCount: number;
  settingsFile: string;
  keybindingsFile: string;
  directory: string;
}

export interface Settings {
  showBuiltinProfiles: boolean;
  showGitStatus: boolean;
  scanEnabled: boolean;
  scanRoots: string[];
  scanDepth: number;
  exclude: string[];
  codeApp: string;
  checkUpdates: boolean;
}

export interface Banner {
  id: number;
  text: string;
  isError: boolean;
}

export interface UiState {
  os: Os;
  editorName: string;
  version: string | null;
  installed: boolean;
  favorites: Row[];
  recent: Row[];
  other: Row[];
  hidden: Row[];
  profiles: ProfileInfo[];
  settings: Settings;
  configError: string | null;
  configPath: string;
  banner: Banner | null;
  launchAtLogin: boolean;
  warnings: string[];
  /** Version of the dev-layout extension bundled with the app. */
  devLayoutVersion: string;
  appVersion: string;
  /** A newer Code Toolbox on GitHub. */
  update: UpdateInfo | null;
}

export interface UpdateInfo {
  version: string;
  /** False where this install can't replace itself: the notice links to the release instead. */
  canInstall: boolean;
  /** Download progress in percent while installing. */
  progress: number | null;
}

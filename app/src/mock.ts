// Fixture state for working on the UI in a plain browser (`npm run dev`, no Tauri): commands are
// logged and the few that change what's shown are applied to the fixture.
import type { Row, UiState } from "./types";

const now = Date.now();
const row = (name: string, detail: string, extra: Partial<Row> = {}): Row => ({
  uri: `file://${detail.replace("~", "/Users/me")}`,
  name,
  baseName: name,
  detail,
  tag: null,
  kind: "folder",
  isWorkspace: false,
  lastOpened: now - 3_600_000,
  profileId: "php",
  hostPath: detail.replace("~", "/Users/me"),
  path: detail.replace("~", "/Users/me"),
  open: false,
  favorite: false,
  agent: null,
  git: { branch: "main", dirty: false },
  ...extra,
});

export const fixture: UiState = {
  os: "macos",
  editorName: "VS Code",
  version: "1.142.0",
  installed: true,
  favorites: [
    row("storefront", "~/Projects/acme/storefront", { favorite: true, open: true, lastOpened: now - 600_000 }),
    row("billing-api", "~/Projects/acme/billing-api", { favorite: true, profileId: "web", git: { branch: "feature/invoices", dirty: true }, lastOpened: now - 86_400_000 }),
  ],
  recent: [
    row("design-system", "~/Projects/acme/design-system", { profileId: "web", open: true, lastOpened: now - 1_800_000 }),
    row("mobile-app", "dev container · ~/Projects/acme/mobile-app", { kind: "devContainer", tag: "dev container", profileId: "web", hostPath: null, git: { branch: "release/2.4", dirty: false }, uri: "vscode-remote://dev-container%2Bx/workspaces/mobile-app", lastOpened: now - 86_400_000 * 2 }),
    row("legacy-cms", "~/Projects/clients/legacy-cms", { git: { branch: "develop", dirty: true }, lastOpened: now - 86_400_000 * 3 }),
    row("build-box", "ssh build-box · /srv/deploy", { kind: "ssh", tag: "ssh", profileId: "__default__profile__", hostPath: null, git: null, uri: "vscode-remote://ssh-remote%2Bbuild-box/srv/deploy", lastOpened: now - 86_400_000 * 6 }),
    row("dotfiles", "~/dotfiles", { profileId: "__default__profile__", lastOpened: now - 86_400_000 * 12 }),
    row("acme", "~/Projects/acme/acme.code-workspace", { kind: "workspaceFile", tag: "workspace", isWorkspace: true, profileId: "web", git: null, lastOpened: now - 86_400_000 * 30 }),
    row("data-pipeline", "~/Projects/acme/data-pipeline", { profileId: "__default__profile__", git: { branch: "main", dirty: false }, lastOpened: now - 86_400_000 * 75 }),
  ],
  other: [],
  hidden: [row("old-prototype", "~/Projects/old-prototype")],
  profiles: [
    { id: "__default__profile__", name: "Default", icon: null, isBuiltin: false, isDefault: true, usesDefaultExtensions: false, extensions: [{ id: "anthropic.claude-code", version: "2.1.3", displayName: "Claude Code" }], projectCount: 3, settingsFile: "/u/settings.json", keybindingsFile: "/u/keybindings.json", directory: "/u" },
    { id: "php", name: "PHP", icon: null, isBuiltin: false, isDefault: false, usesDefaultExtensions: false, extensions: [{ id: "bmewburn.vscode-intelephense-client", version: "1.14", displayName: "PHP Intelephense" }, { id: "xdebug.php-debug", version: "1.3", displayName: null }, { id: "dave-lv.dev-layout", version: "0.1.0", displayName: "Dev Layout (terminal + Claude)" }], projectCount: 3, settingsFile: "/u/p/settings.json", keybindingsFile: "/u/p/keybindings.json", directory: "/u/p" },
    { id: "web", name: "Web", icon: null, isBuiltin: false, isDefault: false, usesDefaultExtensions: false, extensions: [{ id: "dbaeumer.vscode-eslint", version: "3.0", displayName: "ESLint" }, { id: "esbenp.prettier-vscode", version: "11.0", displayName: "Prettier" }, { id: "vue.volar", version: "3.1", displayName: "Vue (Official)" }], projectCount: 4, settingsFile: "/u/w/settings.json", keybindingsFile: "/u/w/keybindings.json", directory: "/u/w" },
    { id: "builtin/agents", name: "Agents", icon: "robot", isBuiltin: true, isDefault: false, usesDefaultExtensions: true, extensions: [], projectCount: 0, settingsFile: "/u/settings.json", keybindingsFile: "/u/keybindings.json", directory: "/u/a" },
  ],
  settings: { showBuiltinProfiles: false, showGitStatus: true, scanEnabled: false, scanRoots: ["~/Documents/Projects"], scanDepth: 2, exclude: ["~/Downloads/**", "~/Library/**"], codeApp: "/Applications/Visual Studio Code.app" },
  configError: null,
  configPath: "~/.config/code-toolbox/config.json",
  banner: null,
  launchAtLogin: false,
  warnings: [],
  devLayoutVersion: "0.2.0",
};

let state = fixture;
let notify: (s: UiState) => void = () => {};
export const onMockState = (f: (s: UiState) => void) => (notify = f);

export async function mockInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  console.log("[mock]", command, args ?? "");
  if (command === "get_state") return state as T;
  if (command === "update_settings") {
    state = { ...state, settings: { ...state.settings, ...(args?.patch as object) } };
    notify(state);
  }
  if (command === "install_dev_layout") {
    const ext = { id: "dave-lv.dev-layout", version: state.devLayoutVersion, displayName: "Dev Layout" };
    const profiles = state.profiles.map((p) =>
      p.id === args?.profileId ? { ...p, extensions: [...p.extensions.filter((e) => e.id !== ext.id), ext] } : p,
    );
    state = { ...state, profiles };
    notify(state);
  }
  if (command === "dismiss_banner") {
    state = { ...state, banner: null };
    notify(state);
  }
  return undefined as T;
}

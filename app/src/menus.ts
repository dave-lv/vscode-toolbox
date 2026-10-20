// The ⋮ / right-click menus: native menus.
import { isTauri } from "@tauri-apps/api/core";
import { Menu } from "@tauri-apps/api/menu";
import { ipc } from "./ipc";
import type { Panel } from "./panel";
import type { Agent, ProfileInfo, Row } from "./types";

const separator = { item: "Separator" as const };

const AGENTS: [Agent | null, string][] = [
  [null, "Profile Setting"],
  ["claude", "Claude Code"],
  ["codex", "Codex"],
];

/** Native menus need Tauri; in a plain browser (mock) they're only logged. */
async function popup(items: Parameters<typeof Menu.new>[0]) {
  if (!isTauri()) return console.log("[mock] menu", items?.items?.map((i) => ("text" in i ? i.text : "—")));
  await (await Menu.new(items)).popup();
}

export async function showProjectMenu(row: Row, panel: Panel) {
  await popup({
    items: [
      { text: "Open", action: () => ipc.openProject(row.uri) },
      { text: "Open in New Window", action: () => ipc.openProject(row.uri, { newWindow: true }) },
      {
        text: "Open in Profile",
        items: panel.visibleProfiles.map((p) => ({
          text: p.id === row.profileId ? `${p.name} ✓` : p.name,
          action: () => ipc.openProject(row.uri, { profileId: p.id }),
        })),
      },
      { text: "Open Without Dev Layout", action: () => ipc.openProject(row.uri, { skipLayout: true }) },
      {
        text: "Dev Layout Agent",
        items: AGENTS.map(([agent, name]) => ({
          text: agent === row.agent ? `${name} ✓` : name,
          action: () => ipc.setAgent(row.uri, agent),
        })),
      },
      ...(row.open ? [{ text: "Close Window", action: () => ipc.closeWindow(row.uri) }] : []),
      separator,
      { text: row.favorite ? "Remove from Favorites" : "Add to Favorites", action: () => ipc.toggleFavorite(row.uri) },
      { text: "Rename…", action: () => panel.setRenaming(row.uri) },
      separator,
      {
        text: panel.revealLabel,
        enabled: row.hostPath !== null,
        action: () => row.hostPath && ipc.reveal(row.hostPath),
      },
      { text: "Copy Path", action: () => ipc.copyText(row.path) },
      separator,
      { text: "Remove from List", action: () => ipc.hide(row.uri) },
    ],
  });
}

export async function showProfileMenu(profile: ProfileInfo, panel: Panel) {
  const expanded = panel.expanded.has(profile.id);
  await popup({
    items: [
      { text: "New Window", action: () => ipc.newWindow(profile.id) },
      {
        text: "Show Projects",
        action: () => {
          // The search was for profiles; it would filter the projects by profile name.
          panel.setQuery("");
          panel.setProfileFilter(profile.id);
          panel.switchTab("projects");
        },
      },
      { text: expanded ? "Hide Extensions" : "Show Extensions", action: () => panel.toggleExpanded(profile.id) },
      separator,
      { text: "Open settings.json", action: () => ipc.openFile(profile.settingsFile, profile.id) },
      { text: "Open keybindings.json", action: () => ipc.openFile(profile.keybindingsFile, profile.id) },
      { text: "Reveal Profile Folder", action: () => ipc.reveal(profile.directory) },
    ],
  });
}

import { useEffect, useRef } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ProfileIcon } from "./components/bits";
import { ipc } from "./ipc";
import { PanelProvider, usePanel, type Tab } from "./panel";
import { useUiState } from "./store";
import { ProfilesView } from "./views/ProfilesView";
import { ProjectsView } from "./views/ProjectsView";
import { SettingsView } from "./views/SettingsView";

export function App() {
  const ui = useUiState();
  if (!ui) return <div className="panel" />;
  return (
    <PanelProvider ui={ui}>
      <PanelView />
    </PanelProvider>
  );
}

function PanelView() {
  const panel = usePanel();
  const search = useRef<HTMLInputElement>(null);
  useKeyboard();

  // Each time the panel is shown: focus the search field with its text selected (kept from last time,
  // but typing replaces it), and report the first painted frame (the "panel open" budget).
  useEffect(() => {
    if (!isTauri()) return;
    const unlisten = listen("panel-shown", () => {
      search.current?.focus();
      search.current?.select();
      requestAnimationFrame(() => requestAnimationFrame(() => ipc.panelPainted()));
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  return (
    <div className="panel">
      <Header />
      <TabBar search={search} />
      <main className="card content">
        {panel.showSettings ? <SettingsView /> : panel.tab === "projects" ? <ProjectsView /> : <ProfilesView />}
      </main>
      {!panel.ui.installed && (
        <div className="banner error">
          <i className="codicon codicon-warning" />
          <span>{panel.ui.editorName} wasn't found, so projects can't be opened.</span>
        </div>
      )}
      <BannerView />
      {panel.picker && <ProfilePicker />}
    </div>
  );
}

function Header() {
  const panel = usePanel();
  return (
    <header className="header">
      <span className="glyph">
        <i className="codicon codicon-code" />
      </span>
      <span className="title">Code Toolbox</span>
      <span className="spacer" />
      {panel.ui.version && (
        <span className="muted small">
          {panel.ui.editorName} {panel.ui.version}
        </span>
      )}
      <button
        className="icon-button"
        title={panel.showSettings ? "Close settings" : `Settings (${panel.modLabel},)`}
        onClick={() => panel.setShowSettings(!panel.showSettings)}
      >
        <i className={`codicon codicon-${panel.showSettings ? "close" : "settings-gear"}`} />
      </button>
    </header>
  );
}

function TabBar({ search }: { search: React.RefObject<HTMLInputElement | null> }) {
  const panel = usePanel();
  const tab = (t: Tab, title: string, key: string) => {
    const active = panel.tab === t && !panel.showSettings;
    return (
      <button className={`tab${active ? " active" : ""}`} title={`${panel.modLabel}${key}`} onClick={() => panel.switchTab(t)}>
        {title}
      </button>
    );
  };
  return (
    <div className="card tabbar">
      {tab("profiles", "Profiles", "1")}
      {tab("projects", "Projects", "2")}
      <span className="spacer" />
      <label className="search">
        <i className="codicon codicon-search" />
        <input
          ref={search}
          placeholder="Search"
          value={panel.query}
          spellCheck={false}
          onChange={(e) => panel.setQuery(e.target.value)}
        />
        {panel.query && (
          <button className="icon-button" title="Clear" onClick={() => panel.setQuery("")}>
            <i className="codicon codicon-close" />
          </button>
        )}
      </label>
    </div>
  );
}

function BannerView() {
  const { ui } = usePanel();
  if (!ui.banner) return null;
  return (
    <div className={`banner${ui.banner.isError ? " error" : ""}`}>
      <i className={`codicon codicon-${ui.banner.isError ? "warning" : "pass-filled"}`} />
      <span className="selectable">{ui.banner.text}</span>
      <button className="icon-button spacer" title="Dismiss" onClick={() => ipc.dismissBanner()}>
        <i className="codicon codicon-close" />
      </button>
    </div>
  );
}

/** ⌘↩ on a project: pick the profile to open it with. */
function ProfilePicker() {
  const panel = usePanel();
  const picker = panel.picker!;
  return (
    <div className="overlay" onClick={() => panel.setPicker(null)}>
      <div className="picker" onClick={(e) => e.stopPropagation()}>
        <div className="picker-title">Open {picker.row.name} in profile</div>
        {panel.visibleProfiles.map((p, i) => (
          <div
            key={p.id}
            className={`picker-row${i === picker.index ? " selected" : ""}`}
            onMouseEnter={() => panel.setPicker({ ...picker, index: i })}
            onClick={() => {
              panel.setPicker(null);
              ipc.openProject(picker.row.uri, { profileId: p.id });
            }}
          >
            <ProfileIcon profile={p} small />
            <span>{p.name}</span>
            <span className="spacer" />
            {p.id === picker.row.profileId && <i className="codicon codicon-check muted" />}
          </div>
        ))}
      </div>
    </div>
  );
}

/** Keyboard: ↑/↓ move, ↩ opens (⌘↩ picks a profile), Esc clears/closes, ⌘1/⌘2 tabs, ⌘, settings. */
function useKeyboard() {
  const panel = usePanel();
  const ref = useRef(panel);
  ref.current = panel;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const p = ref.current;
      if (p.renaming) return; // the rename field handles its own keys
      const mod = p.isMod(e);
      if (mod && (e.key === "1" || e.key === "2" || e.key === ",")) {
        e.preventDefault();
        if (e.key === ",") p.setShowSettings(true);
        else p.switchTab(e.key === "1" ? "profiles" : "projects");
        return;
      }
      if (p.showSettings) {
        if (e.key === "Escape") p.setShowSettings(false);
        return;
      }
      const ids = p.navigableIds;
      const at = p.selection ? ids.indexOf(p.selection) : -1;
      switch (e.key) {
        case "ArrowDown":
        case "ArrowUp": {
          e.preventDefault();
          const delta = e.key === "ArrowDown" ? 1 : -1;
          if (p.picker) {
            const index = Math.max(0, Math.min(p.visibleProfiles.length - 1, p.picker.index + delta));
            p.setPicker({ ...p.picker, index });
          } else if (ids.length) {
            p.setSelection(ids[Math.max(0, Math.min(ids.length - 1, at + delta))]);
          }
          return;
        }
        case "Enter": {
          e.preventDefault();
          if (p.picker) {
            const profile = p.visibleProfiles[p.picker.index];
            if (profile) ipc.openProject(p.picker.row.uri, { profileId: profile.id });
            p.setPicker(null);
          } else if (p.selection && p.tab === "projects") {
            const row = p.sections.flatMap((s) => s.rows).find((r) => r.uri === p.selection);
            if (!row) return;
            if (mod) {
              const index = Math.max(0, p.visibleProfiles.findIndex((x) => x.id === row.profileId));
              p.setPicker({ row, index });
            } else {
              ipc.openProject(row.uri);
            }
          } else if (p.selection && p.tab === "profiles") {
            ipc.newWindow(p.selection);
          }
          return;
        }
        case "Escape":
          e.preventDefault();
          if (p.picker) p.setPicker(null);
          else if (p.query) p.setQuery("");
          else ipc.hidePanel();
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

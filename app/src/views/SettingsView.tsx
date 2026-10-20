import { useEffect, useState } from "react";
import { Toggle } from "../components/bits";
import { ipc } from "../ipc";
import { usePanel } from "../panel";
import type { ProfileInfo } from "../types";

const DEV_LAYOUT_ID = "dave-lv.dev-layout";
// Its id before 0.3.0; installing replaces it, so it counts as an old version.
const DEV_LAYOUT_OLD_ID = "mdri.dev-layout";

const lines = (text: string) =>
  text
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);

export function SettingsView() {
  const { ui } = usePanel();
  const s = ui.settings;
  const savedRoots = s.scanRoots.join(", ");
  const savedExclude = s.exclude.join("\n");
  const [roots, setRoots] = useState(savedRoots);
  const [exclude, setExclude] = useState(savedExclude);
  const [codeApp, setCodeApp] = useState(s.codeApp);

  // Follow changes saved elsewhere (config.json edited by hand, the other app), but not every state
  // update: those arrive while typing too.
  useEffect(() => setRoots(savedRoots), [savedRoots]);
  useEffect(() => setExclude(savedExclude), [savedExclude]);
  useEffect(() => setCodeApp(s.codeApp), [s.codeApp]);

  return (
    <div className="scroll settings">
      {ui.configError && (
        <div className="config-error">
          <div>{ui.configError}</div>
          <div className="muted">
            Settings changes aren't saved until it's fixed. Reset keeps the old file as config.json.broken.
          </div>
          <div className="buttons">
            <button onClick={() => ipc.openConfig()}>Edit config.json</button>
            <button onClick={() => ipc.resetConfig()}>Reset config.json</button>
          </div>
        </div>
      )}

      <Toggle label="Launch at login" checked={ui.launchAtLogin} onChange={(on) => ipc.setLaunchAtLogin(on)} />
      <Toggle
        label="Show built-in profiles (Agents)"
        checked={s.showBuiltinProfiles}
        onChange={(on) => ipc.updateSettings({ showBuiltinProfiles: on })}
      />
      <Toggle
        label="Show git branch and uncommitted changes"
        checked={s.showGitStatus}
        onChange={(on) => ipc.updateSettings({ showGitStatus: on })}
      />
      <div className="group">
        <Toggle
          label={`List projects never opened in ${ui.editorName}`}
          checked={s.scanEnabled}
          onChange={(on) => ipc.updateSettings({ scanEnabled: on })}
        />
        {s.scanEnabled && (
          <label className="field">
            <span>Folders to scan (comma-separated, {s.scanDepth} levels deep)</span>
            <input
              value={roots}
              onChange={(e) => setRoots(e.target.value)}
              onBlur={() => ipc.updateSettings({ scanRoots: roots.split(",") })}
              onKeyDown={(e) => e.key === "Enter" && ipc.updateSettings({ scanRoots: roots.split(",") })}
            />
          </label>
        )}
      </div>

      <label className="field">
        <span>Hide projects matching (one glob per line)</span>
        <textarea
          rows={3}
          value={exclude}
          spellCheck={false}
          onChange={(e) => setExclude(e.target.value)}
          onBlur={() => ipc.updateSettings({ exclude: lines(exclude) })}
        />
      </label>

      {ui.os === "macos" && (
        <label className="field">
          <span>VS Code app</span>
          <input
            value={codeApp}
            spellCheck={false}
            onChange={(e) => setCodeApp(e.target.value)}
            onBlur={() => ipc.updateSettings({ codeApp })}
            onKeyDown={(e) => e.key === "Enter" && ipc.updateSettings({ codeApp })}
          />
        </label>
      )}

      <DevLayoutExtension />

      <div className="buttons">
        <button onClick={() => ipc.importToolboxFavorites()}>Import Toolbox Favorites</button>
      </div>
      <div className="buttons">
        <button onClick={() => ipc.openConfig()}>Edit config.json</button>
      </div>

      {ui.hidden.length > 0 && (
        <div className="group">
          <span className="muted small">Removed from list</span>
          {ui.hidden.map((row) => (
            <div key={row.uri} className="line hidden-row">
              <span>{row.name}</span>
              <span className="detail">{row.detail}</span>
              <span className="spacer" />
              <button className="link" onClick={() => ipc.unhide(row.uri)}>
                Restore
              </button>
            </div>
          ))}
        </div>
      )}

      <hr />
      <div className="line">
        <span className="faint small selectable">Config: {ui.configPath}</span>
        <span className="spacer" />
        <button onClick={() => ipc.quit()}>Quit</button>
      </div>
    </div>
  );
}

/** Installs the dev-layout extension bundled with the app into a profile, or updates it there. */
function DevLayoutExtension() {
  const { ui } = usePanel();
  const [picked, setPicked] = useState<string | null>(null);
  const bundled = ui.devLayoutVersion;
  const profiles = ui.profiles.filter((p) => !p.isBuiltin);
  const installed = (p: ProfileInfo) =>
    p.extensions.find((e) => e.id.toLowerCase() === DEV_LAYOUT_ID)?.version ??
    (p.extensions.some((e) => e.id.toLowerCase() === DEV_LAYOUT_OLD_ID) ? "0" : undefined);
  // Unless picked: a profile with an older copy, else the one with the most projects.
  const profile =
    profiles.find((p) => p.id === picked) ??
    profiles.find((p) => installed(p) && isNewer(bundled, installed(p)!)) ??
    [...profiles].sort((a, b) => b.projectCount - a.projectCount)[0];
  if (!profile || !bundled) return null;

  const version = installed(profile);
  const action = !version ? "Install" : isNewer(bundled, version) ? `Update to ${bundled}` : "Reinstall";
  return (
    <div className="field">
      <span>Dev Layout extension: a shell and Claude Code or Codex in every project window</span>
      <div className="buttons">
        <select value={profile.id} onChange={(e) => setPicked(e.target.value)}>
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
              {installed(p) === "0" ? " (old version installed)" : installed(p) ? ` (${installed(p)} installed)` : ""}
            </option>
          ))}
        </select>
        <button onClick={() => ipc.installDevLayout(profile.id)}>{action}</button>
      </div>
    </div>
  );
}

/** `a` is a later x.y.z version than `b`. */
function isNewer(a: string, b: string) {
  const [pa, pb] = [a.split(".").map(Number), b.split(".").map(Number)];
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d) return d > 0;
  }
  return false;
}

// The panel's own UI state (tab, search, selection, …) and what's derived from it. App-wide state
// lives in Rust (UiState); this is only what the panel shows and where the keyboard focus is.
import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { fuzzyScore } from "./lib/format";
import type { ProfileInfo, Row, UiState } from "./types";

export type Tab = "projects" | "profiles";
export type SectionKind = "favorites" | "recent" | "other" | "results";

export interface Section {
  kind: SectionKind;
  title: string;
  rows: Row[];
}

export interface Panel {
  ui: UiState;
  tab: Tab;
  switchTab: (tab: Tab) => void;
  showSettings: boolean;
  setShowSettings: (on: boolean) => void;
  query: string;
  setQuery: (q: string) => void;
  selection: string | null;
  setSelection: (id: string | null) => void;
  profileFilter: string | null;
  setProfileFilter: (id: string | null) => void;
  isCollapsed: (kind: SectionKind) => boolean;
  toggleCollapsed: (kind: SectionKind) => void;
  renaming: string | null;
  setRenaming: (uri: string | null) => void;
  picker: { row: Row; index: number } | null;
  setPicker: (p: { row: Row; index: number } | null) => void;
  expanded: Set<string>;
  toggleExpanded: (id: string) => void;
  sections: Section[];
  /** Built-in profiles only when Settings says so. */
  visibleProfiles: ProfileInfo[];
  filteredProfiles: ProfileInfo[];
  /** What ↑/↓ moves through on the current tab. */
  navigableIds: string[];
  profileOf: (row: Row) => ProfileInfo | undefined;
  /** ⌘ on macOS, Ctrl elsewhere. */
  isMod: (e: { metaKey: boolean; ctrlKey: boolean }) => boolean;
  modLabel: string;
  revealLabel: string;
}

const PanelContext = createContext<Panel | null>(null);

export function usePanel(): Panel {
  const panel = useContext(PanelContext);
  if (!panel) throw new Error("usePanel outside PanelProvider");
  return panel;
}

export function PanelProvider({ ui, children }: { ui: UiState; children: ReactNode }) {
  const [tab, setTab] = useState<Tab>("projects");
  const [showSettings, setShowSettings] = useState(false);
  const [query, setQueryRaw] = useState("");
  const [selection, setSelection] = useState<string | null>(null);
  const [profileFilter, setProfileFilter] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [renaming, setRenaming] = useState<string | null>(null);
  const [picker, setPicker] = useState<{ row: Row; index: number } | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const visibleProfiles = useMemo(
    () => ui.profiles.filter((p) => !p.isBuiltin || ui.settings.showBuiltinProfiles),
    [ui.profiles, ui.settings.showBuiltinProfiles],
  );
  const filteredProfiles = useMemo(
    () => (query.trim() ? visibleProfiles.filter((p) => fuzzyScore(query, p.name, "") !== null) : visibleProfiles),
    [visibleProfiles, query],
  );

  const sections = useMemo((): Section[] => {
    let { favorites, recent, other } = ui;
    if (profileFilter) {
      favorites = favorites.filter((r) => r.profileId === profileFilter);
      recent = recent.filter((r) => r.profileId === profileFilter);
      other = [];
    }
    const q = query.trim();
    if (q) {
      const results = [...favorites, ...recent, ...other]
        .map((row) => ({ row, score: fuzzyScore(q, row.name, row.detail) }))
        .filter((r): r is { row: Row; score: number } => r.score !== null)
        .sort(
          (a, b) =>
            a.score - b.score ||
            Number(b.row.favorite) - Number(a.row.favorite) ||
            (b.row.lastOpened ?? 0) - (a.row.lastOpened ?? 0),
        )
        .map((r) => r.row);
      return [{ kind: "results", title: results.length ? "Results" : "No matches", rows: results }];
    }
    const s: Section[] = [];
    if (favorites.length) s.push({ kind: "favorites", title: "Favorites", rows: favorites });
    if (recent.length) s.push({ kind: "recent", title: "Recent", rows: recent });
    if (other.length) s.push({ kind: "other", title: `Not opened in ${ui.editorName}`, rows: other });
    return s;
  }, [ui, profileFilter, query]);

  const isCollapsed = (kind: SectionKind) => (kind === "recent" || kind === "other") && !!collapsed[kind];

  const navigableIds = useMemo(
    () =>
      tab === "projects"
        ? sections.flatMap((s) => (isCollapsed(s.kind) ? [] : s.rows.map((r) => r.uri)))
        : filteredProfiles.map((p) => p.id),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [tab, sections, filteredProfiles, collapsed],
  );

  const mac = ui.os === "macos";
  const panel: Panel = {
    ui,
    tab,
    switchTab: (t) => {
      setTab(t);
      setShowSettings(false);
      setSelection(null);
    },
    showSettings,
    setShowSettings,
    query,
    setQuery: (q) => {
      setQueryRaw(q);
      setSelection(null);
    },
    selection: selection && navigableIds.includes(selection) ? selection : (navigableIds[0] ?? null),
    setSelection,
    profileFilter,
    setProfileFilter,
    isCollapsed,
    toggleCollapsed: (kind) => setCollapsed((c) => ({ ...c, [kind]: !c[kind] })),
    renaming,
    setRenaming,
    picker,
    setPicker,
    expanded,
    toggleExpanded: (id) =>
      setExpanded((s) => {
        const next = new Set(s);
        if (!next.delete(id)) next.add(id);
        return next;
      }),
    sections,
    visibleProfiles,
    filteredProfiles,
    navigableIds,
    profileOf: (row) => ui.profiles.find((p) => p.id === row.profileId),
    isMod: (e) => (mac ? e.metaKey : e.ctrlKey),
    modLabel: mac ? "⌘" : "Ctrl+",
    revealLabel: mac ? "Reveal in Finder" : ui.os === "windows" ? "Show in Explorer" : "Show in File Manager",
  };
  return <PanelContext.Provider value={panel}>{children}</PanelContext.Provider>;
}

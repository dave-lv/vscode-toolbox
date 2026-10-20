import { useEffect, useRef, useState } from "react";
import { Avatar, GitLabel, ProfileBadge, RowMenuButton, SectionHeader, Tag } from "../components/bits";
import { ipc } from "../ipc";
import { relativeDate } from "../lib/format";
import { showProjectMenu } from "../menus";
import { usePanel } from "../panel";
import type { Row } from "../types";

export function ProjectsView() {
  const panel = usePanel();
  const filter = panel.profileFilter ? panel.ui.profiles.find((p) => p.id === panel.profileFilter) : undefined;

  return (
    <div className="scroll">
      {filter && (
        <div className="filter-chip">
          <span className="muted">Profile:</span>
          <ProfileBadge profile={filter} />
          <button className="link spacer" onClick={() => panel.setProfileFilter(null)}>
            Show all
          </button>
        </div>
      )}
      {panel.sections.map((section, i) => {
        const collapsible = section.kind === "recent" || section.kind === "other";
        return (
          <section key={section.kind}>
            <SectionHeader
              title={section.title}
              collapsed={collapsible ? panel.isCollapsed(section.kind) : undefined}
              onToggle={collapsible ? () => panel.toggleCollapsed(section.kind) : undefined}
              legend={i === 0}
            />
            {!panel.isCollapsed(section.kind) &&
              section.rows.map((row) => <ProjectRow key={row.uri} row={row} reorderable={section.kind === "favorites"} />)}
          </section>
        );
      })}
      {panel.sections.length === 0 && (
        <p className="empty">No projects yet. Open a folder in {panel.ui.editorName} and it shows up here.</p>
      )}
    </div>
  );
}

function ProjectRow({ row, reorderable }: { row: Row; reorderable: boolean }) {
  const panel = usePanel();
  const selected = panel.selection === row.uri;
  const renaming = panel.renaming === row.uri;
  const profile = panel.profileOf(row);
  const ref = useRef<HTMLDivElement>(null);
  const [dropTarget, setDropTarget] = useState(false);

  useEffect(() => {
    if (selected) ref.current?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  const drag = reorderable
    ? {
        draggable: !renaming,
        onDragStart: (e: React.DragEvent) => {
          e.dataTransfer.setData("text/x-favorite", row.uri);
          e.dataTransfer.effectAllowed = "move";
        },
        onDragOver: (e: React.DragEvent) => {
          if (e.dataTransfer.types.includes("text/x-favorite")) {
            e.preventDefault();
            setDropTarget(true);
          }
        },
        onDragLeave: () => setDropTarget(false),
        onDrop: (e: React.DragEvent) => {
          setDropTarget(false);
          const uri = e.dataTransfer.getData("text/x-favorite");
          if (!uri || uri === row.uri) return;
          // The dragged favorite takes this row's place: its index in the full list. Inserted at that index
          // in the list without the dragged one, it lands after this row when moving down, before it
          // when moving up.
          ipc.moveFavorite(uri, panel.ui.favorites.findIndex((f) => f.uri === row.uri));
        },
      }
    : {};

  return (
    <div
      ref={ref}
      className={`row${selected ? " selected" : ""}${dropTarget ? " drop-target" : ""}`}
      onClick={(e) => {
        if (renaming) return;
        panel.setSelection(row.uri);
        ipc.openProject(row.uri, { newWindow: panel.isMod(e) });
      }}
      onContextMenu={(e) => {
        e.preventDefault();
        showProjectMenu(row, panel);
      }}
      {...drag}
    >
      <Avatar name={row.name} />
      <div className="row-text">
        <div className="line">
          {renaming ? <RenameField row={row} /> : <span className="name">{row.name}</span>}
          {row.tag && <Tag text={row.tag} />}
          {row.open && <span className="open-dot" title={`Has an open ${panel.ui.editorName} window`} />}
          <span className="spacer" />
          {row.git && <GitLabel info={row.git} />}
          {row.lastOpened !== null && <span className="date">{relativeDate(row.lastOpened)}</span>}
        </div>
        <div className="line">
          {profile && <ProfileBadge profile={profile} />}
          <span className="detail" title={row.detail}>
            {row.detail}
          </span>
        </div>
      </div>
      <RowMenuButton onOpen={() => showProjectMenu(row, panel)} />
    </div>
  );
}

function RenameField({ row }: { row: Row }) {
  const panel = usePanel();
  const [draft, setDraft] = useState(row.name);
  return (
    <input
      className="rename"
      autoFocus
      value={draft}
      onFocus={(e) => e.target.select()}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => panel.setRenaming(null)}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") {
          ipc.rename(row.uri, draft);
          panel.setRenaming(null);
        } else if (e.key === "Escape") {
          panel.setRenaming(null);
        }
      }}
    />
  );
}

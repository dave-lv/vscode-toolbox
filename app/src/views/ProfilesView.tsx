import { useEffect, useRef } from "react";
import { ProfileIcon, RowMenuButton, SectionHeader, Tag } from "../components/bits";
import { ipc } from "../ipc";
import { plural } from "../lib/format";
import { showProfileMenu } from "../menus";
import { usePanel } from "../panel";
import type { ExtensionInfo, ProfileInfo } from "../types";

export function ProfilesView() {
  const panel = usePanel();
  return (
    <div className="scroll">
      <SectionHeader title={`${panel.ui.editorName} profiles`} />
      {panel.filteredProfiles.map((p) => (
        <ProfileRow key={p.id} profile={p} />
      ))}
      {panel.filteredProfiles.length === 0 && <p className="empty">No matching profiles</p>}
    </div>
  );
}

function ProfileRow({ profile }: { profile: ProfileInfo }) {
  const panel = usePanel();
  const selected = panel.selection === profile.id;
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (selected) ref.current?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  const extensions = profile.usesDefaultExtensions
    ? "Default's extensions"
    : plural(profile.extensions.length, "extension");

  return (
    <div>
      <div
        ref={ref}
        className={`row profile-row${selected ? " selected" : ""}${profile.isBuiltin ? " dim" : ""}`}
        title={`Open a new window with the ${profile.name} profile`}
        onClick={() => {
          panel.setSelection(profile.id);
          ipc.newWindow(profile.id);
        }}
        onContextMenu={(e) => {
          e.preventDefault();
          showProfileMenu(profile, panel);
        }}
      >
        <ProfileIcon profile={profile} />
        <div className="row-text">
          <div className="line">
            <span className="name">{profile.name}</span>
            {profile.isBuiltin && <Tag text="built-in" />}
          </div>
          <div className="line detail">
            {extensions} · {plural(profile.projectCount, "project")}
          </div>
        </div>
        <RowMenuButton onOpen={() => showProfileMenu(profile, panel)} />
      </div>
      {panel.expanded.has(profile.id) && <ExtensionList extensions={profile.extensions} />}
    </div>
  );
}

function ExtensionList({ extensions }: { extensions: ExtensionInfo[] }) {
  return (
    <div className="extensions">
      {extensions.length === 0 && <div className="muted">No extensions</div>}
      {extensions.map((e) => (
        <div key={e.id} className="line" title={e.id}>
          <span className="ellipsis">{e.displayName ?? e.id}</span>
          <span className="spacer" />
          <span className="faint">{e.version}</span>
        </div>
      ))}
    </div>
  );
}

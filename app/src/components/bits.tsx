// Small building blocks shared by the views (Components.swift in the Swift app).
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { GitInfo, ProfileInfo } from "../types";
import { initials, profileCode, tileColor } from "../lib/format";

/** Coloured tile with the project's initials, squarish like VS Code's profile badge. */
export function Avatar({ name }: { name: string }) {
  return (
    <div className="avatar" style={{ background: tileColor(name) }}>
      {initials(name)}
    </div>
  );
}

/** The profile's codicon, or its short name, in a badge. */
export function ProfileIcon({ profile, small = false }: { profile: ProfileInfo; small?: boolean }) {
  return (
    <span className={`profile-glyph${profile.isDefault ? " default" : ""}${small ? " small" : ""}`}>
      {profile.icon ? <i className={`codicon codicon-${profile.icon}`} /> : profileCode(profile.name)}
    </span>
  );
}

/** The profile's name as a small outlined label, shown under each project. */
export function ProfileBadge({ profile }: { profile: ProfileInfo }) {
  return <span className="profile-badge">{profile.name}</span>;
}

export function Tag({ text }: { text: string }) {
  return <span className="tag">{text}</span>;
}

export function GitLabel({ info }: { info: GitInfo }) {
  return (
    <span className="git" title={info.dirty ? `${info.branch}, uncommitted changes` : info.branch}>
      <i className="codicon codicon-git-branch" />
      <span className="git-branch">{info.branch}</span>
      {info.dirty && <span className="dirty-dot" />}
    </span>
  );
}

/**
 * "?" with a popover explaining the dots and labels on project rows. The popover is rendered on <body>
 * with fixed positioning: inside the scrolling list it would be clipped.
 */
export function Legend() {
  const [at, setAt] = useState<{ top: number; right: number } | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const popover = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!at) return;
    const close = (e: Event) => {
      const t = e.target as Node;
      if (!button.current?.contains(t) && !popover.current?.contains(t)) setAt(null);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("scroll", close, true); // it doesn't follow the list
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("scroll", close, true);
    };
  }, [at]);
  const toggle = () => {
    const r = button.current?.getBoundingClientRect();
    setAt(at || !r ? null : { top: r.bottom + 4, right: window.innerWidth - r.right });
  };
  return (
    <>
      <button ref={button} className="icon-button" title="What the dots mean" onClick={toggle}>
        <i className="codicon codicon-question" />
      </button>
      {at &&
        createPortal(
          <div ref={popover} className="popover" style={{ top: at.top, right: at.right }}>
            <div className="legend-row">
              <span className="legend-icon">
                <span className="open-dot" />
              </span>
              The project has an open editor window
            </div>
            <div className="legend-row">
              <span className="legend-icon git">
                <i className="codicon codicon-git-branch" />
                <span className="dirty-dot" />
              </span>
              Git branch; the orange dot means uncommitted changes
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}

export function SectionHeader(props: {
  title: string;
  collapsed?: boolean;
  onToggle?: () => void;
  legend?: boolean;
}) {
  return (
    <div className="section-header" onClick={props.onToggle} style={props.onToggle ? { cursor: "pointer" } : undefined}>
      {props.collapsed !== undefined && (
        <i className={`codicon codicon-chevron-down chevron${props.collapsed ? " collapsed" : ""}`} />
      )}
      <span>{props.title}</span>
      {props.legend && (
        <span className="spacer legend" onClick={(e) => e.stopPropagation()}>
          <Legend />
        </span>
      )}
    </div>
  );
}

/** Switch-style checkbox. */
export function Toggle(props: { label: string; checked: boolean; onChange: (on: boolean) => void }) {
  return (
    <label className="toggle">
      <span>{props.label}</span>
      <input type="checkbox" checked={props.checked} onChange={(e) => props.onChange(e.target.checked)} />
      <span className="switch" />
    </label>
  );
}

/** Vertical ⋮ button that opens the row's (native) menu. */
export function RowMenuButton({ onOpen }: { onOpen: () => void }) {
  return (
    <button
      className="icon-button row-menu"
      title="More"
      onClick={(e) => {
        e.stopPropagation();
        onOpen();
      }}
    >
      <i className="codicon codicon-kebab-vertical" />
    </button>
  );
}

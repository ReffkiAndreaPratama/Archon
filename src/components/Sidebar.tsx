import {
  Archive as ArchiveIcon,
  ChevronRight,
  FolderOpen,
  FolderPlus,
  Layers3,
  X,
} from "lucide-react";

import { BrandMark } from "./BrandMark";
import { FileGlyph } from "./FileGlyph";
import { shortPath } from "../lib/format";
import type { RecentArchive } from "../types/archive";

export type AppView = "archive" | "create" | "formats";

interface SidebarProps {
  view: AppView;
  recent: RecentArchive[];
  onViewChange: (view: AppView) => void;
  onOpen: () => void;
  onOpenRecent: (path: string) => void;
  onRemoveRecent: (path: string) => void;
  onClearRecent: () => void;
}

const NAV_ITEMS: Array<{
  view: AppView;
  label: string;
  hint: string;
  icon: typeof ArchiveIcon;
}> = [
  { view: "archive", label: "Archive", hint: "Browse and extract", icon: ArchiveIcon },
  { view: "create", label: "Create", hint: "Build a new ZIP", icon: FolderPlus },
  { view: "formats", label: "Formats", hint: "Engine capabilities", icon: Layers3 },
];

export function Sidebar({
  view,
  recent,
  onViewChange,
  onOpen,
  onOpenRecent,
  onRemoveRecent,
  onClearRecent,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <BrandMark size={36} />
        <div className="brand-copy">
          <strong>ARCHON</strong>
          <span>Archive manager</span>
        </div>
      </div>

      <button
        type="button"
        className="open-archive-button"
        onClick={onOpen}
        aria-label="Open archive"
        title="Open archive (Ctrl+O)"
      >
        <FolderOpen size={17} />
        <span>Open archive</span>
        <kbd>Ctrl O</kbd>
      </button>

      <nav className="sidebar-nav" aria-label="Workspace">
        <span className="nav-label">Workspace</span>
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const active = view === item.view;
          return (
            <button
              key={item.view}
              type="button"
              className={`nav-item ${active ? "is-active" : ""}`}
              onClick={() => onViewChange(item.view)}
              aria-current={active ? "page" : undefined}
              aria-label={`${item.label} — ${item.hint}`}
              title={item.hint}
            >
              <span className="nav-icon">
                <Icon size={17} />
              </span>
              <span className="nav-copy">
                <strong>{item.label}</strong>
                <small>{item.hint}</small>
              </span>
              <ChevronRight size={14} className="nav-chevron" />
            </button>
          );
        })}
      </nav>

      <section className="recent-section" aria-labelledby="recent-heading">
        <div className="recent-heading">
          <span className="nav-label" id="recent-heading">
            Recent
          </span>
          {recent.length > 0 ? (
            <button type="button" className="text-button" onClick={onClearRecent}>
              Clear
            </button>
          ) : null}
        </div>

        {recent.length === 0 ? (
          <p className="recent-empty">Archives you open or create will be listed here.</p>
        ) : (
          <div className="recent-list">
            {recent.map((item) => (
              <div className="recent-row" key={item.path}>
                <button
                  type="button"
                  className="recent-main"
                  onClick={() => onOpenRecent(item.path)}
                  title={item.path}
                >
                  <FileGlyph name={item.name} size={16} />
                  <span className="recent-copy">
                    <strong>{item.name}</strong>
                    <small>{shortPath(item.path, 32)}</small>
                  </span>
                </button>
                <button
                  type="button"
                  className="recent-remove"
                  onClick={() => onRemoveRecent(item.path)}
                  aria-label={`Remove ${item.name} from recent archives`}
                  title="Remove from recent"
                >
                  <X size={13} />
                </button>
              </div>
            ))}
          </div>
        )}
      </section>

      <div className="sidebar-footer">
        <div className="phase-card">
          <div className="phase-card-top">
            <span className="phase-chip">Early access</span>
            <span className="phase-dot" aria-hidden="true" />
          </div>
          <strong>Archive core online</strong>
          <p>ZIP write · 7Z/TAR read · verify</p>
        </div>
        <div className="sidebar-meta">
          <span>Archon v0.1.0</span>
          <span>MIT</span>
        </div>
      </div>
    </aside>
  );
}

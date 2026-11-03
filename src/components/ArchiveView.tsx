import {
  ArrowDown,
  ArrowUp,
  ChevronRight,
  ChevronUp,
  FileArchive,
  FileWarning,
  FolderDown,
  FolderOpen,
  Home,
  LockKeyhole,
  Search,
  ShieldCheck,
  Square,
  SquareCheckBig,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { KeyboardEvent, MouseEvent } from "react";

import {
  basename,
  crcHex,
  cx,
  formatBytes,
  formatDateTime,
  formatPercent,
  pluralize,
  shortPath,
} from "../lib/format";
import {
  breadcrumbSegments,
  buildArchiveTree,
  childrenRows,
  findNode,
  nodeToRow,
  searchRows,
  sortRows,
  type BrowserRow,
  type SortDirection,
  type SortKey,
} from "../lib/tree";
import type { ArchiveListing } from "../types/archive";
import { FileGlyph } from "./FileGlyph";

interface ArchiveViewProps {
  listing: ArchiveListing;
  busy: boolean;
  onTest: () => void;
  onExtract: (selection: string[]) => void;
  onClose: () => void;
  onReveal: () => void;
}

function selectedFileCount(listing: ArchiveListing, paths: string[]): number {
  if (paths.length === 0) {
    return listing.entries.filter((entry) => !entry.isDir).length;
  }
  return listing.entries.filter(
    (entry) =>
      !entry.isDir &&
      paths.some((path) => entry.path === path || entry.path.startsWith(`${path}/`)),
  ).length;
}

function savedPercent(totalSize: number, totalCompressed: number): number {
  if (totalSize <= 0) return 0;
  return Math.max(0, Math.min(100, ((totalSize - totalCompressed) / totalSize) * 100));
}

export function ArchiveView({
  listing,
  busy,
  onTest,
  onExtract,
  onClose,
  onReveal,
}: ArchiveViewProps) {
  const [currentPath, setCurrentPath] = useState("");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortDirection, setSortDirection] = useState<SortDirection>("asc");

  const tree = useMemo(() => buildArchiveTree(listing.entries), [listing.entries]);
  const folderRows = useMemo(
    () => childrenRows(tree, currentPath),
    [tree, currentPath],
  );
  const matches = useMemo(
    () => (query.trim() ? searchRows(tree, query) : []),
    [tree, query],
  );
  const baseRows = query.trim() ? matches : folderRows;
  const rows = useMemo(
    () => sortRows(baseRows, sortKey, sortDirection),
    [baseRows, sortKey, sortDirection],
  );

  const selectedRows = useMemo(
    () =>
      Array.from(selected)
        .map((path) => findNode(tree, path))
        .filter((node): node is NonNullable<typeof node> => Boolean(node))
        .map(nodeToRow),
    [selected, tree],
  );
  const selectedCount = selectedRows.length;
  const selectedBytes = selectedRows.reduce((total, row) => total + row.size, 0);
  const selectedFiles = selectedFileCount(listing, Array.from(selected));
  const allSelected = rows.length > 0 && rows.every((row) => selected.has(row.path));
  const someSelected = rows.some((row) => selected.has(row.path));
  const currentFolder = findNode(tree, currentPath);
  const fileCount = listing.entries.filter((entry) => !entry.isDir).length;
  const folderCount = listing.entries.filter((entry) => entry.isDir).length;
  const savings = savedPercent(listing.totalSize, listing.totalCompressed);
  const segments = breadcrumbSegments(currentPath);
  const isSearching = Boolean(query.trim());

  const navigate = (path: string) => {
    setCurrentPath(path);
    setSelected(new Set());
    setQuery("");
  };

  const goUp = () => {
    if (!currentPath) return;
    const parent = currentPath.split("/").slice(0, -1).join("/");
    navigate(parent);
  };

  const toggleRow = (path: string) => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleRowClick = (row: BrowserRow, event: MouseEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey) {
      toggleRow(row.path);
      return;
    }
    setSelected(new Set([row.path]));
  };

  const handleRowKey = (row: BrowserRow, event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (row.isDir && event.key === "Enter") navigate(row.path);
      else toggleRow(row.path);
    }
  };

  const toggleAll = () => {
    setSelected((previous) => {
      const next = new Set(previous);
      if (allSelected) {
        for (const row of rows) next.delete(row.path);
      } else {
        for (const row of rows) next.add(row.path);
      }
      return next;
    });
  };

  const changeSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDirection((value) => (value === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDirection(key === "name" ? "asc" : "desc");
    }
  };

  const sortIcon = (key: SortKey) => {
    if (sortKey !== key) return null;
    return sortDirection === "asc" ? <ArrowUp size={12} /> : <ArrowDown size={12} />;
  };

  return (
    <div className="archive-view">
      <section className="archive-hero">
        <div className="archive-identity">
          <span className="archive-format-icon">
            <FileArchive size={26} />
            <span>{listing.formatLabel}</span>
          </span>
          <div className="archive-title">
            <div className="archive-title-top">
              <h1>{basename(listing.path)}</h1>
              <span className="format-pill">{listing.formatLabel}</span>
            </div>
            <p className="archive-path" title={listing.path}>
              {shortPath(listing.path, 78)}
            </p>
          </div>
        </div>

        <div className="archive-actions">
          <button
            type="button"
            className="icon-button"
            onClick={onReveal}
            title="Show archive in Explorer"
            aria-label="Show archive in Explorer"
          >
            <FolderOpen size={17} />
          </button>
          <button
            type="button"
            className="btn btn-ghost"
            onClick={onTest}
            disabled={busy}
            title="Read every entry and verify checksums"
          >
            <ShieldCheck size={16} />
            Test
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => onExtract(Array.from(selected))}
            disabled={busy}
          >
            <FolderDown size={16} />
            {selectedCount > 0 ? `Extract ${selectedFiles}` : "Extract"}
          </button>
          <button
            type="button"
            className="icon-button"
            onClick={onClose}
            title="Close archive"
            aria-label="Close archive"
          >
            <X size={17} />
          </button>
        </div>

        <div className="archive-stats">
          <div className="stat">
            <span>Archive size</span>
            <strong>{formatBytes(listing.fileSize)}</strong>
          </div>
          <div className="stat">
            <span>Unpacked</span>
            <strong>{formatBytes(listing.totalSize)}</strong>
          </div>
          <div className="stat">
            <span>Space saved</span>
            <strong>{formatPercent(savings)}</strong>
          </div>
          <div className="stat">
            <span>Contents</span>
            <strong>
              {fileCount} {pluralize(fileCount, "file")} · {folderCount}{" "}
              {pluralize(folderCount, "folder")}
            </strong>
          </div>
          <div className="stat">
            <span>Encryption</span>
            <strong className={listing.encryptedEntries > 0 ? "is-accent" : undefined}>
              {listing.encryptedEntries > 0
                ? `${listing.encryptedEntries} encrypted`
                : "None"}
            </strong>
          </div>
        </div>

        {listing.warnings.length > 0 ? (
          <div className="archive-warning">
            <FileWarning size={16} />
            <div>
              <strong>Archive warnings</strong>
              <span>{listing.warnings.join(" ")}</span>
            </div>
          </div>
        ) : null}
      </section>

      <div className="archive-browser">
        <div className="browser-main">
          <div className="browser-toolbar">
            <div className="browser-path">
              {currentPath ? (
                <button
                  type="button"
                  className="icon-button compact"
                  onClick={goUp}
                  title="Go to parent folder"
                  aria-label="Go to parent folder"
                >
                  <ChevronUp size={16} />
                </button>
              ) : null}
              <nav className="breadcrumb" aria-label="Archive location">
                <button
                  type="button"
                  className={cx("breadcrumb-root", currentPath === "" && !isSearching && "is-current")}
                  onClick={() => navigate("")}
                  title="Archive root"
                >
                  <Home size={14} />
                  <span>Root</span>
                </button>
                {segments.map((segment, index) => (
                  <span className="breadcrumb-part" key={segment.path}>
                    <ChevronRight size={13} />
                    <button
                      type="button"
                      className={index === segments.length - 1 && !isSearching ? "is-current" : ""}
                      onClick={() => navigate(segment.path)}
                    >
                      {segment.name}
                    </button>
                  </span>
                ))}
                {isSearching ? (
                  <span className="breadcrumb-part">
                    <ChevronRight size={13} />
                    <span className="breadcrumb-query">Search</span>
                  </span>
                ) : null}
              </nav>
            </div>

            <div className="browser-tools">
              <label className="search-box">
                <Search size={15} />
                <input
                  value={query}
                  placeholder="Find in archive"
                  spellCheck={false}
                  onChange={(event) => {
                    setQuery(event.target.value);
                    setSelected(new Set());
                  }}
                  aria-label="Find entries in archive"
                />
                {query ? (
                  <button
                    type="button"
                    className="search-clear"
                    onClick={() => {
                      setQuery("");
                      setSelected(new Set());
                    }}
                    aria-label="Clear search"
                    title="Clear search"
                  >
                    <X size={13} />
                  </button>
                ) : null}
              </label>
              <span className="toolbar-selection">
                {isSearching
                  ? `${matches.length} ${pluralize(matches.length, "match", "matches")}`
                  : `${folderRows.length} ${pluralize(folderRows.length, "item")}`}
              </span>
            </div>
          </div>

          <div className="archive-table" role="table" aria-label="Archive contents">
            <div className="table-head" role="row">
              <div className="check-cell" role="columnheader">
                <button
                  type="button"
                  className={cx("check-button", someSelected && !allSelected && "is-mixed")}
                  onClick={toggleAll}
                  aria-label={allSelected ? "Clear selection" : "Select all visible entries"}
                  title={allSelected ? "Clear selection" : "Select all visible entries"}
                >
                  {allSelected ? (
                    <SquareCheckBig size={15} />
                  ) : someSelected ? (
                    <span className="mixed-mark" />
                  ) : (
                    <Square size={15} />
                  )}
                </button>
              </div>
              <button
                type="button"
                className="table-head-cell name-cell is-sortable"
                onClick={() => changeSort("name")}
                role="columnheader"
              >
                Name {sortIcon("name")}
              </button>
              <button
                type="button"
                className="table-head-cell align-right is-sortable"
                onClick={() => changeSort("size")}
                role="columnheader"
              >
                Size {sortIcon("size")}
              </button>
              <button
                type="button"
                className="table-head-cell align-right is-sortable"
                onClick={() => changeSort("compressed")}
                role="columnheader"
              >
                Packed {sortIcon("compressed")}
              </button>
              <div className="table-head-cell align-right" role="columnheader">
                Saved
              </div>
              <button
                type="button"
                className="table-head-cell is-sortable"
                onClick={() => changeSort("modified")}
                role="columnheader"
              >
                Modified {sortIcon("modified")}
              </button>
              <div className="table-head-cell lock-cell" role="columnheader">
                <LockKeyhole size={13} />
              </div>
            </div>

            <div className="table-body" role="rowgroup">
              {rows.length === 0 ? (
                <div className="table-empty">
                  <span className="empty-icon">
                    {isSearching ? <Search size={24} /> : <FolderOpen size={24} />}
                  </span>
                  <strong>
                    {isSearching
                      ? "No entries match that search"
                      : currentFolder && currentFolder.children.size === 0
                        ? "This folder is empty"
                        : "Nothing to show"}
                  </strong>
                  <p>
                    {isSearching
                      ? "Try a shorter name or a different part of the path."
                      : "The archive may only contain explicit folder entries."}
                  </p>
                  {isSearching ? (
                    <button
                      type="button"
                      className="btn btn-ghost"
                      onClick={() => {
                        setQuery("");
                        setSelected(new Set());
                      }}
                    >
                      Clear search
                    </button>
                  ) : null}
                </div>
              ) : (
                rows.map((row) => {
                  const isSelected = selected.has(row.path);
                  return (
                    <div
                      className={cx("table-row", isSelected && "is-selected")}
                      key={row.key}
                      role="row"
                      tabIndex={0}
                      onClick={(event) => handleRowClick(row, event)}
                      onDoubleClick={() => {
                        if (row.isDir) navigate(row.path);
                      }}
                      onKeyDown={(event) => handleRowKey(row, event)}
                      aria-selected={isSelected}
                    >
                      <div className="check-cell" role="cell">
                        <button
                          type="button"
                          className="check-button"
                          onClick={(event) => {
                            event.stopPropagation();
                            toggleRow(row.path);
                          }}
                          aria-label={`${isSelected ? "Deselect" : "Select"} ${row.name}`}
                          title={isSelected ? "Deselect" : "Select"}
                        >
                          {isSelected ? <SquareCheckBig size={15} /> : <Square size={15} />}
                        </button>
                      </div>
                      <div className="table-cell name-cell" role="cell">
                        <FileGlyph
                          name={row.name}
                          isDir={row.isDir}
                          open={row.isDir && isSelected}
                        />
                        <span className="name-copy">
                          <span className="entry-name" title={row.path}>
                            {row.name}
                            {row.isDir ? "/" : ""}
                          </span>
                          {isSearching ? (
                            <span className="entry-subpath">{row.path}</span>
                          ) : row.isDir ? (
                            <span className="entry-subpath">
                              Folder · {row.fileCount} {pluralize(row.fileCount, "file")}
                            </span>
                          ) : null}
                        </span>
                        {row.isSymlink ? (
                          <span className="inline-flag" title="Symbolic link entries are skipped">
                            link
                          </span>
                        ) : null}
                      </div>
                      <div className="table-cell align-right mono" role="cell">
                        {formatBytes(row.size)}
                      </div>
                      <div className="table-cell align-right mono muted-cell" role="cell">
                        {formatBytes(row.compressedSize)}
                      </div>
                      <div className="table-cell align-right" role="cell">
                        <span
                          className={cx(
                            "ratio-value",
                            row.ratio >= 55 && "is-good",
                            row.ratio > 0 && row.ratio < 20 && "is-low",
                          )}
                        >
                          {row.isDir ? "—" : formatPercent(row.ratio)}
                        </span>
                      </div>
                      <div className="table-cell modified-cell" role="cell">
                        {formatDateTime(row.modified)}
                      </div>
                      <div className="table-cell lock-cell" role="cell">
                        {row.encrypted ? (
                          <LockKeyhole
                            size={13}
                            className="tone-accent"
                            aria-label="Encrypted"
                          />
                        ) : null}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>

          <div className="browser-footer">
            <span>
              {isSearching
                ? `${rows.length} ${pluralize(rows.length, "entry", "entries")} across the archive`
                : `${rows.length} ${pluralize(rows.length, "item")} in this folder`}
            </span>
            <span>
              {selectedCount > 0
                ? `${selectedCount} selected · ${formatBytes(selectedBytes)}`
                : "Ctrl-click to select multiple items"}
            </span>
          </div>
        </div>

        <aside className="inspector" aria-label="Archive details">
          <header className="inspector-header">
            <div>
              <span className="eyebrow">Inspector</span>
              <h2>
                {selectedCount === 1
                  ? selectedRows[0].isDir
                    ? "Folder"
                    : "File details"
                  : selectedCount > 1
                    ? "Selection"
                    : "Archive profile"}
              </h2>
            </div>
            {selectedCount > 0 ? (
              <button
                type="button"
                className="text-button"
                onClick={() => setSelected(new Set())}
              >
                Clear
              </button>
            ) : null}
          </header>

          <div className="inspector-body">
            {selectedCount === 0 ? (
              <ArchiveProfile listing={listing} />
            ) : selectedCount === 1 ? (
              <EntryProfile
                row={selectedRows[0]}
                listingPath={listing.path}
                onExtract={() => onExtract(Array.from(selected))}
              />
            ) : (
              <div className="multi-profile">
                <div className="multi-count">{selectedCount}</div>
                <h3>items selected</h3>
                <p>
                  {selectedFiles} {pluralize(selectedFiles, "file")} will be extracted. Folders
                  include their contents.
                </p>
                <div className="detail-grid">
                  <div>
                    <span>Total size</span>
                    <strong>{formatBytes(selectedBytes)}</strong>
                  </div>
                  <div>
                    <span>Folders</span>
                    <strong>{selectedRows.filter((row) => row.isDir).length}</strong>
                  </div>
                  <div>
                    <span>Files</span>
                    <strong>{selectedRows.filter((row) => !row.isDir).length}</strong>
                  </div>
                  <div>
                    <span>Encrypted</span>
                    <strong>{selectedRows.filter((row) => row.encrypted).length}</strong>
                  </div>
                </div>
                <button
                  type="button"
                  className="btn btn-outline inspector-action"
                  onClick={() => onExtract(Array.from(selected))}
                >
                  <FolderDown size={16} />
                  Extract selection
                </button>
              </div>
            )}
          </div>
        </aside>
      </div>
    </div>
  );
}

function ArchiveProfile({ listing }: { listing: ArchiveListing }) {
  const files = listing.entries.filter((entry) => !entry.isDir).length;
  const folders = listing.entries.length - files;
  return (
    <div className="archive-profile">
      <div className="profile-format">
        <span className="profile-format-icon">
          <FileArchive size={23} />
        </span>
        <div>
          <strong>{listing.formatLabel}</strong>
          <span>Archive profile</span>
        </div>
      </div>

      <div className="detail-grid">
        <div>
          <span>Archive size</span>
          <strong>{formatBytes(listing.fileSize)}</strong>
        </div>
        <div>
          <span>Unpacked</span>
          <strong>{formatBytes(listing.totalSize)}</strong>
        </div>
        <div>
          <span>Packed</span>
          <strong>{formatBytes(listing.totalCompressed)}</strong>
        </div>
        <div>
          <span>Space saved</span>
          <strong>{formatPercent(savedPercent(listing.totalSize, listing.totalCompressed))}</strong>
        </div>
        <div>
          <span>Files</span>
          <strong>{files}</strong>
        </div>
        <div>
          <span>Folders</span>
          <strong>{folders}</strong>
        </div>
        <div>
          <span>Encrypted</span>
          <strong>{listing.encryptedEntries}</strong>
        </div>
        <div>
          <span>Format</span>
          <strong>{listing.format}</strong>
        </div>
      </div>

      {listing.comment ? (
        <div className="profile-comment">
          <span>Archive comment</span>
          <p>{listing.comment}</p>
        </div>
      ) : null}

      <div className="profile-note">
        <ShieldCheck size={15} />
        <p>
          Extraction is atomic per file. Unsafe paths, Windows device names and symlinks are
          rejected before anything is written.
        </p>
      </div>

      {listing.warnings.length > 0 ? (
        <div className="profile-warnings">
          <span>Warnings</span>
          {listing.warnings.map((warning) => (
            <p key={warning}>{warning}</p>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function EntryProfile({
  row,
  listingPath,
  onExtract,
}: {
  row: BrowserRow;
  listingPath: string;
  onExtract: () => void;
}) {
  return (
    <div className="entry-profile">
      <div className="entry-identity">
        <span className="entry-glyph-large">
          <FileGlyph name={row.name} isDir={row.isDir} size={24} />
        </span>
        <div>
          <h3 title={row.path}>{row.name}</h3>
          <span>{row.isDir ? "Folder" : row.method || "File"}</span>
        </div>
      </div>

      <div className="path-block">
        <span>Archive path</span>
        <code>/{row.path}{row.isDir ? "/" : ""}</code>
      </div>

      <div className="detail-grid">
        <div>
          <span>{row.isDir ? "Contents size" : "Size"}</span>
          <strong>{formatBytes(row.size)}</strong>
        </div>
        <div>
          <span>Packed</span>
          <strong>{formatBytes(row.compressedSize)}</strong>
        </div>
        <div>
          <span>Saved</span>
          <strong>{row.isDir ? "—" : formatPercent(row.ratio)}</strong>
        </div>
        <div>
          <span>Modified</span>
          <strong>{formatDateTime(row.modified)}</strong>
        </div>
        {row.isDir ? (
          <div>
            <span>Files inside</span>
            <strong>{row.fileCount}</strong>
          </div>
        ) : (
          <div>
            <span>CRC-32</span>
            <strong className="mono">{crcHex(row.crc32)}</strong>
          </div>
        )}
        <div>
          <span>Encrypted</span>
          <strong>{row.encrypted ? "Yes — AES/ZipCrypto" : "No"}</strong>
        </div>
      </div>

      <div className="profile-note subtle">
        <LockKeyhole size={15} />
        <p>
          Archive source: <span title={listingPath}>{shortPath(listingPath, 36)}</span>
        </p>
      </div>

      {row.isSymlink ? (
        <div className="profile-warning-inline">
          Symbolic link entries are listed but never recreated during extraction.
        </div>
      ) : null}

      <button type="button" className="btn btn-outline inspector-action" onClick={onExtract}>
        <FolderDown size={16} />
        Extract selection
      </button>
    </div>
  );
}

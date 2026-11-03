import { open } from "@tauri-apps/plugin-dialog";
import { Eye, EyeOff, FolderDown, FolderOpen } from "lucide-react";
import { useMemo, useState } from "react";

import { defaultExtractPath, formatBytes, pluralize } from "../lib/format";
import type { ArchiveListing, OverwritePolicy } from "../types/archive";
import type { ExtractFormValues } from "../types/ui";
import { Modal } from "./Modal";

interface ExtractDialogProps {
  listing: ArchiveListing;
  selection: string[];
  initialPassword: string;
  onConfirm: (values: ExtractFormValues) => void;
  onClose: () => void;
}

const OVERWRITE_OPTIONS: Array<{
  value: OverwritePolicy;
  label: string;
  detail: string;
}> = [
  { value: "rename", label: "Rename", detail: "Keep both copies" },
  { value: "overwrite", label: "Replace", detail: "Overwrite existing files" },
  { value: "skip", label: "Skip", detail: "Leave existing files untouched" },
];

function countSelected(listing: ArchiveListing, paths: string[]) {
  let files = 0;
  let dirs = 0;
  for (const entry of listing.entries) {
    const included = paths.some(
      (path) => entry.path === path || entry.path.startsWith(`${path}/`),
    );
    if (!included) continue;
    if (entry.isDir) dirs += 1;
    else files += 1;
  }
  return { files, dirs };
}

function countLabel({ files, dirs }: { files: number; dirs: number }): string {
  const parts = [`${files} ${pluralize(files, "file")}`];
  if (dirs > 0) parts.push(`${dirs} ${pluralize(dirs, "folder")}`);
  return parts.join(", ");
}

export function ExtractDialog({
  listing,
  selection,
  initialPassword,
  onConfirm,
  onClose,
}: ExtractDialogProps) {
  const [mode, setMode] = useState<"selected" | "all">(
    selection.length > 0 ? "selected" : "all",
  );
  const [destination, setDestination] = useState(defaultExtractPath(listing.path));
  const [overwrite, setOverwrite] = useState<OverwritePolicy>("rename");
  const [preserveStructure, setPreserveStructure] = useState(true);
  const [password, setPassword] = useState(initialPassword);
  const [showPassword, setShowPassword] = useState(false);

  const selectedCount = useMemo(
    () => countSelected(listing, selection),
    [listing, selection],
  );
  const totalCount = useMemo(() => {
    let files = 0;
    let dirs = 0;
    for (const entry of listing.entries) {
      if (entry.isDir) dirs += 1;
      else files += 1;
    }
    return { files, dirs };
  }, [listing]);
  const target = mode === "selected" ? selectedCount : totalCount;
  const targetEntries = target.files + target.dirs;
  const targetLabel = countLabel(target);
  const needsPassword = listing.encryptedEntries > 0;

  const browse = async () => {
    try {
      const selected = await open({
        title: "Choose extraction folder",
        directory: true,
        multiple: false,
        defaultPath: destination,
      });
      if (typeof selected === "string") setDestination(selected);
    } catch {
      // The dialog plugin can be unavailable outside the desktop shell.
    }
  };

  const confirm = () => {
    if (!destination.trim()) return;
    onConfirm({
      destination: destination.trim(),
      entries: mode === "selected" ? selection : undefined,
      password: password || undefined,
      overwrite,
      preserveStructure,
    });
  };

  return (
    <Modal
      title="Extract archive"
      description={`${targetLabel} from ${listing.formatLabel}`}
      icon={<FolderDown size={19} />}
      onClose={onClose}
      width="medium"
      footer={
        <>
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={confirm}
            disabled={!destination.trim() || targetEntries === 0}
          >
            <FolderDown size={16} />
            Extract
          </button>
        </>
      }
    >
      <div className="dialog-section">
        <span className="field-label">Contents</span>
        <div className="segmented" role="radiogroup" aria-label="Contents to extract">
          <button
            type="button"
            role="radio"
            aria-checked={mode === "selected"}
            className={mode === "selected" ? "is-active" : undefined}
            disabled={selection.length === 0}
            onClick={() => setMode("selected")}
          >
            Selection
            <span>
              {selection.length > 0 ? countLabel(selectedCount) : "Nothing selected"}
            </span>
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={mode === "all"}
            className={mode === "all" ? "is-active" : undefined}
            onClick={() => setMode("all")}
          >
            Whole archive
            <span>{countLabel(totalCount)}</span>
          </button>
        </div>
      </div>

      <div className="field">
        <label htmlFor="extract-destination">Destination folder</label>
        <div className="input-with-action">
          <input
            id="extract-destination"
            value={destination}
            spellCheck={false}
            onChange={(event) => setDestination(event.target.value)}
          />
          <button
            type="button"
            className="input-action"
            onClick={() => void browse()}
            aria-label="Browse for destination folder"
            title="Browse for destination folder"
          >
            <FolderOpen size={16} />
          </button>
        </div>
      </div>

      <div className="dialog-section">
        <span className="field-label">If a file already exists</span>
        <div className="choice-grid" role="radiogroup" aria-label="Overwrite policy">
          {OVERWRITE_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={overwrite === option.value}
              className={`choice-card ${overwrite === option.value ? "is-active" : ""}`}
              onClick={() => setOverwrite(option.value)}
            >
              <span>{option.label}</span>
              <small>{option.detail}</small>
            </button>
          ))}
        </div>
      </div>

      <label className="check-row">
        <input
          type="checkbox"
          checked={preserveStructure}
          onChange={(event) => setPreserveStructure(event.target.checked)}
        />
        <span>
          Preserve folder structure
          <small>Turn off to place every extracted file in the destination root.</small>
        </span>
      </label>

      {needsPassword ? (
        <div className="field">
          <label htmlFor="extract-password">Archive password</label>
          <div className="input-with-action">
            <input
              id="extract-password"
              type={showPassword ? "text" : "password"}
              value={password}
              autoComplete="off"
              spellCheck={false}
              placeholder="Required for encrypted entries"
              onChange={(event) => setPassword(event.target.value)}
            />
            <button
              type="button"
              className="input-action"
              onClick={() => setShowPassword((value) => !value)}
              aria-label={showPassword ? "Hide password" : "Show password"}
              title={showPassword ? "Hide password" : "Show password"}
            >
              {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
            </button>
          </div>
          <p className="field-hint">
            {listing.encryptedEntries} encrypted{" "}
            {pluralize(listing.encryptedEntries, "entry", "entries")}. You will be prompted if
            this is left empty.
          </p>
        </div>
      ) : null}

      <div className="security-note">
        <span>Path safety</span>
        <p>
          Every target is validated before writing. Absolute paths, parent traversal, Windows
          device names and symbolic links are rejected.
        </p>
      </div>

      <p className="dialog-footnote">
        Selected folders include their contents. Uncompressed size:{" "}
        {formatBytes(listing.totalSize)}.
      </p>
    </Modal>
  );
}

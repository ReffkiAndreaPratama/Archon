import { save } from "@tauri-apps/plugin-dialog";
import {
  Archive,
  ChevronDown,
  Eye,
  EyeOff,
  FileArchive,
  FilePlus2,
  FolderOpen,
  FolderPlus,
  Gauge,
  KeyRound,
  LockKeyhole,
  Plus,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  X,
  Zap,
} from "lucide-react";
import { useState } from "react";

import {
  basename,
  pluralize,
  shortPath,
} from "../lib/format";
import type { CompressionPreset } from "../types/archive";
import type { CreateDraft } from "../types/ui";
import { FileGlyph } from "./FileGlyph";

interface CreateViewProps {
  draft: CreateDraft;
  busy: boolean;
  dragging: boolean;
  onChange: (patch: Partial<CreateDraft>) => void;
  onAddFiles: () => void;
  onAddFolders: () => void;
  onRemoveSource: (path: string) => void;
  onCreate: () => void;
}

const PRESETS: Array<{
  value: CompressionPreset;
  label: string;
  hint: string;
  icon: typeof Archive;
}> = [
  { value: "store", label: "Store", hint: "No compression", icon: Archive },
  { value: "fast", label: "Fast", hint: "Deflate 1", icon: Zap },
  { value: "normal", label: "Normal", hint: "Deflate 6", icon: SlidersHorizontal },
  { value: "maximum", label: "Maximum", hint: "Deflate 9", icon: Gauge },
  { value: "ultra", label: "Ultra", hint: "Deflate 9", icon: Sparkles },
];

function passwordScore(password: string): number {
  if (!password) return 0;
  let score = 0;
  if (password.length >= 8) score += 1;
  if (password.length >= 12) score += 1;
  if (/[a-z]/.test(password) && /[A-Z]/.test(password)) score += 1;
  if (/\d/.test(password)) score += 1;
  if (/[^A-Za-z0-9]/.test(password)) score += 1;
  return Math.min(4, score);
}

const STRENGTH_LABELS = ["Too short", "Weak", "Fair", "Strong", "Very strong"];

function guessedSourceIsDir(path: string): boolean {
  const name = basename(path);
  return !name.includes(".");
}

export function CreateView({
  draft,
  busy,
  dragging,
  onChange,
  onAddFiles,
  onAddFolders,
  onRemoveSource,
  onCreate,
}: CreateViewProps) {
  const [excludeInput, setExcludeInput] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const totalKnown = draft.sources.length;
  const strength = passwordScore(draft.password);
  const passwordsMatch =
    draft.password.length > 0 && draft.password === draft.confirmPassword;
  const encryptionReady =
    !draft.encrypt || (draft.password.length > 0 && passwordsMatch);

  const chooseOutput = async () => {
    try {
      const selected = await save({
        title: "Create ZIP archive",
        defaultPath: draft.output || basename(draft.sources[0]?.path ?? "archive.zip"),
        filters: [{ name: "ZIP archive", extensions: ["zip"] }],
      });
      if (selected) onChange({ output: selected });
    } catch {
      setLocalError("The save dialog could not be opened.");
    }
  };

  const addExclude = () => {
    const pattern = excludeInput.trim();
    if (!pattern || draft.exclude.includes(pattern)) {
      setExcludeInput("");
      return;
    }
    onChange({ exclude: [...draft.exclude, pattern] });
    setExcludeInput("");
  };

  const submit = () => {
    if (draft.sources.length === 0) {
      setLocalError("Add at least one file or folder.");
      return;
    }
    if (!draft.output.trim()) {
      setLocalError("Choose an output archive path.");
      return;
    }
    if (draft.encrypt && !draft.password) {
      setLocalError("Enter a password or turn off encryption.");
      return;
    }
    if (draft.encrypt && draft.password !== draft.confirmPassword) {
      setLocalError("The passwords do not match.");
      return;
    }
    setLocalError(null);
    onCreate();
  };

  const selectedPreset = PRESETS.find((preset) => preset.value === draft.preset);

  return (
    <div className="create-view">
      <div className="create-main">
        <header className="view-heading">
          <span className="eyebrow">New archive</span>
          <h1>Build a ZIP archive</h1>
          <p>
            Choose sources, set compression and optionally encrypt with AES-256. Archon scans
            and packs them with the same Rust engine used by the CLI.
          </p>
        </header>

        {localError ? (
          <div className="inline-error" role="alert">
            {localError}
            <button
              type="button"
              className="icon-button compact"
              onClick={() => setLocalError(null)}
              aria-label="Dismiss error"
            >
              <X size={14} />
            </button>
          </div>
        ) : null}

        <section className={`form-card ${dragging ? "is-dragging" : ""}`}>
          <header className="form-card-heading">
            <div className="form-card-title">
              <span className="step-number">01</span>
              <div>
                <h2>Sources</h2>
                <p>Files and folders are stored with their names and relative structure.</p>
              </div>
            </div>
            <div className="form-card-actions">
              <button type="button" className="btn btn-ghost btn-small" onClick={onAddFiles}>
                <FilePlus2 size={15} />
                Add files
              </button>
              <button type="button" className="btn btn-ghost btn-small" onClick={onAddFolders}>
                <FolderPlus size={15} />
                Add folder
              </button>
            </div>
          </header>

          {draft.sources.length === 0 ? (
            <button type="button" className="source-dropzone" onClick={onAddFiles}>
              <span className="source-dropzone-icon">
                <FolderOpen size={24} />
              </span>
              <strong>{dragging ? "Drop sources to add them" : "No sources yet"}</strong>
              <p>Drop files or folders anywhere in the window, or click to browse.</p>
            </button>
          ) : (
            <div className="source-list">
              {draft.sources.map((source, index) => (
                <div className="source-row" key={`${source.path}-${index}`}>
                  <FileGlyph
                    name={basename(source.path)}
                    isDir={source.isDir || guessedSourceIsDir(source.path)}
                    size={17}
                  />
                  <div className="source-copy">
                    <strong>{basename(source.path)}</strong>
                    <span title={source.path}>{shortPath(source.path, 72)}</span>
                  </div>
                  <span className="source-kind">
                    {source.isDir || guessedSourceIsDir(source.path) ? "Folder" : "File"}
                  </span>
                  <button
                    type="button"
                    className="icon-button compact"
                    onClick={() => onRemoveSource(source.path)}
                    aria-label={`Remove ${basename(source.path)}`}
                    title="Remove source"
                  >
                    <X size={14} />
                  </button>
                </div>
              ))}
            </div>
          )}

          {draft.sources.length > 0 ? (
            <div className="source-footer">
              <span>
                {totalKnown} {pluralize(totalKnown, "source")} queued
              </span>
              <button
                type="button"
                className="text-button tone-danger"
                onClick={() => onChange({ sources: [] })}
              >
                <Trash2 size={13} />
                Clear all
              </button>
            </div>
          ) : null}
        </section>

        <section className="form-card">
          <header className="form-card-heading">
            <div className="form-card-title">
              <span className="step-number">02</span>
              <div>
                <h2>Destination</h2>
                <p>Archon writes to a temporary file, then moves it into place.</p>
              </div>
            </div>
          </header>

          <div className="field">
            <label htmlFor="create-output">Output archive</label>
            <div className="input-with-action">
              <input
                id="create-output"
                value={draft.output}
                placeholder="Choose where to save the .zip"
                spellCheck={false}
                onChange={(event) => {
                  onChange({ output: event.target.value });
                  setLocalError(null);
                }}
              />
              <button
                type="button"
                className="input-action input-action-wide"
                onClick={() => void chooseOutput()}
              >
                <FolderOpen size={15} />
                Browse
              </button>
            </div>
            <p className="field-hint">A .zip extension is added if you omit one.</p>
          </div>

          <label className="check-row compact">
            <input
              type="checkbox"
              checked={draft.overwriteOutput}
              onChange={(event) => onChange({ overwriteOutput: event.target.checked })}
            />
            <span>
              Replace an existing file
              <small>Without this, Archon stops instead of overwriting.</small>
            </span>
          </label>
        </section>

        <section className="form-card">
          <header className="form-card-heading">
            <div className="form-card-title">
              <span className="step-number">03</span>
              <div>
                <h2>Compression</h2>
                <p>Presets choose a Deflate level. Store skips compression entirely.</p>
              </div>
            </div>
          </header>

          <div className="preset-grid">
            {PRESETS.map((preset) => {
              const Icon = preset.icon;
              const active = draft.preset === preset.value;
              return (
                <button
                  type="button"
                  key={preset.value}
                  className={`preset-card ${active ? "is-active" : ""}`}
                  onClick={() => {
                    onChange({ preset: preset.value });
                    setLocalError(null);
                  }}
                  aria-pressed={active}
                >
                  <Icon size={17} />
                  <strong>{preset.label}</strong>
                  <span>{preset.hint}</span>
                </button>
              );
            })}
          </div>
          {draft.preset === "ultra" ? (
            <p className="field-hint">
              Ultra uses Deflate level 9. ZIP has no higher compression tier, so the engine will
              report this in its warnings.
            </p>
          ) : null}

          <button
            type="button"
            className={`advanced-toggle ${advancedOpen ? "is-open" : ""}`}
            onClick={() => setAdvancedOpen((value) => !value)}
            aria-expanded={advancedOpen}
          >
            <SlidersHorizontal size={14} />
            Advanced compression
            <ChevronDown size={14} />
          </button>

          {advancedOpen ? (
            <div className="advanced-panel">
              <div className="field">
                <label htmlFor="create-method">Method</label>
                <select
                  id="create-method"
                  value={draft.method}
                  onChange={(event) =>
                    onChange({
                      method: event.target.value as CreateDraft["method"],
                    })
                  }
                >
                  <option value="auto">Auto — use the selected preset</option>
                  <option value="deflate">Deflate — choose level</option>
                  <option value="store">Store — no compression</option>
                </select>
              </div>

              {draft.method === "deflate" ? (
                <div className="field">
                  <div className="field-label-row">
                    <label htmlFor="create-level">Deflate level</label>
                    <span className="level-value">{draft.level}</span>
                  </div>
                  <input
                    id="create-level"
                    className="range-input"
                    type="range"
                    min={1}
                    max={9}
                    step={1}
                    value={draft.level}
                    onChange={(event) =>
                      onChange({ level: Number(event.target.value) })
                    }
                  />
                  <div className="range-labels">
                    <span>Faster</span>
                    <span>Smaller</span>
                  </div>
                </div>
              ) : null}

              {draft.method !== "auto" ? (
                <p className="field-hint">
                  An explicit method overrides the compression preset.
                </p>
              ) : null}
            </div>
          ) : null}
        </section>

        <section className="form-card">
          <header className="form-card-heading">
            <div className="form-card-title">
              <span className="step-number">04</span>
              <div>
                <h2>Encryption</h2>
                <p>Encrypt file contents with AES-256 (AE-2) inside the ZIP container.</p>
              </div>
            </div>
            <label className="switch">
              <input
                type="checkbox"
                checked={draft.encrypt}
                onChange={(event) =>
                  onChange({
                    encrypt: event.target.checked,
                    password: event.target.checked ? draft.password : "",
                    confirmPassword: event.target.checked ? draft.confirmPassword : "",
                  })
                }
                aria-label="Encrypt archive with AES-256"
              />
              <span />
            </label>
          </header>

          {draft.encrypt ? (
            <div className="encryption-panel">
              <div className="field">
                <label htmlFor="create-password">Password</label>
                <div className="input-with-action">
                  <input
                    id="create-password"
                    type={showPassword ? "text" : "password"}
                    value={draft.password}
                    autoComplete="new-password"
                    spellCheck={false}
                    placeholder="Use a long, unique password"
                    onChange={(event) => {
                      onChange({ password: event.target.value });
                      setLocalError(null);
                    }}
                  />
                  <button
                    type="button"
                    className="input-action"
                    onClick={() => setShowPassword((value) => !value)}
                    aria-label={showPassword ? "Hide password" : "Show password"}
                  >
                    {showPassword ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                </div>
              </div>

              <div className="strength-meter" aria-hidden="true">
                {[0, 1, 2, 3].map((index) => (
                  <span
                    key={index}
                    className={index < strength ? `is-filled strength-${strength}` : ""}
                  />
                ))}
              </div>
              <span className="strength-label">
                {draft.password ? STRENGTH_LABELS[strength] : "AES-256 encryption enabled"}
              </span>

              <div className="field">
                <label htmlFor="create-password-confirm">Confirm password</label>
                <input
                  id="create-password-confirm"
                  type={showPassword ? "text" : "password"}
                  value={draft.confirmPassword}
                  autoComplete="new-password"
                  spellCheck={false}
                  placeholder="Repeat the password"
                  onChange={(event) => {
                    onChange({ confirmPassword: event.target.value });
                    setLocalError(null);
                  }}
                />
                {draft.confirmPassword && !passwordsMatch ? (
                  <p className="field-error">Passwords do not match.</p>
                ) : null}
              </div>

              <div className="security-note warning-note">
                <span>
                  <KeyRound size={14} /> No recovery
                </span>
                <p>
                  Archon does not store this password. If it is lost, the encrypted contents
                  cannot be recovered.
                </p>
              </div>
            </div>
          ) : (
            <div className="disabled-note">
              <LockKeyhole size={16} />
              <span>Encryption is off. File contents will be readable by anyone with the ZIP.</span>
            </div>
          )}
        </section>

        <section className="form-card">
          <header className="form-card-heading">
            <div className="form-card-title">
              <span className="step-number">05</span>
              <div>
                <h2>Exclude patterns</h2>
                <p>Optional globs matched against archive paths and file names.</p>
              </div>
            </div>
          </header>

          <div className="exclude-add">
            <input
              value={excludeInput}
              placeholder="e.g. *.tmp or node_modules/**"
              spellCheck={false}
              onChange={(event) => setExcludeInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  addExclude();
                }
              }}
            />
            <button
              type="button"
              className="btn btn-ghost"
              onClick={addExclude}
              disabled={!excludeInput.trim()}
            >
              <Plus size={15} />
              Add
            </button>
          </div>

          {draft.exclude.length > 0 ? (
            <div className="exclude-list">
              {draft.exclude.map((pattern) => (
                <span className="exclude-chip" key={pattern}>
                  <code>{pattern}</code>
                  <button
                    type="button"
                    onClick={() =>
                      onChange({
                        exclude: draft.exclude.filter((value) => value !== pattern),
                      })
                    }
                    aria-label={`Remove exclude pattern ${pattern}`}
                  >
                    <X size={12} />
                  </button>
                </span>
              ))}
            </div>
          ) : (
            <p className="field-hint">No exclude patterns. Every source entry will be packed.</p>
          )}
        </section>
      </div>

      <aside className="create-summary">
        <div className="summary-card">
          <div className="summary-top">
            <span className="summary-icon">
              <FileArchive size={22} />
            </span>
            <span className="eyebrow">Archive summary</span>
          </div>
          <h2>{draft.output ? basename(draft.output) : "Untitled archive"}</h2>
          <p className="summary-path" title={draft.output}>
            {draft.output ? shortPath(draft.output, 42) : "Choose a destination to continue"}
          </p>

          <div className="summary-rows">
            <div>
              <span>Format</span>
              <strong>ZIP</strong>
            </div>
            <div>
              <span>Sources</span>
              <strong>
                {draft.sources.length} {pluralize(draft.sources.length, "item")}
              </strong>
            </div>
            <div>
              <span>Compression</span>
              <strong>
                {draft.method === "store"
                  ? "Store"
                  : draft.method === "deflate"
                    ? `Deflate ${draft.level}`
                    : (selectedPreset?.label ?? "Normal")}
              </strong>
            </div>
            <div>
              <span>Encryption</span>
              <strong className={draft.encrypt ? "tone-accent" : undefined}>
                {draft.encrypt ? "AES-256" : "None"}
              </strong>
            </div>
            <div>
              <span>Excludes</span>
              <strong>{draft.exclude.length}</strong>
            </div>
            <div>
              <span>Replace output</span>
              <strong>{draft.overwriteOutput ? "Yes" : "No"}</strong>
            </div>
          </div>

          <button
            type="button"
            className="btn btn-primary btn-large summary-create"
            onClick={submit}
            disabled={busy || draft.sources.length === 0 || !draft.output.trim() || !encryptionReady}
          >
            <FileArchive size={18} />
            {busy ? "Packing…" : "Create archive"}
          </button>

          <p className="summary-note">
            {draft.encrypt
              ? "File contents are protected. Entry names remain visible in the ZIP directory."
              : "Archon validates paths and never follows symlinks while scanning sources."}
          </p>
        </div>

        <div className="summary-assurance">
          <ShieldCheck size={16} />
          <div>
            <strong>Safe output</strong>
            <span>
              Writes to a sibling temporary file first. Existing archives are removed only after
              the new ZIP is complete.
            </span>
          </div>
        </div>

        <div className="summary-assurance">
          <Gauge size={16} />
          <div>
            <strong>Measured after packing</strong>
            <span>
              The engine reports the final archive size, compression ratio and elapsed time when
              creation completes. No estimate is shown beforehand.
            </span>
          </div>
        </div>
      </aside>
    </div>
  );
}

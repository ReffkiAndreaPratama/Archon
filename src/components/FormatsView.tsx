import {
  AlertTriangle,
  Check,
  FileArchive,
  KeyRound,
  Layers3,
  LockKeyhole,
  Minus,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";

import type { FormatCapability } from "../types/archive";

interface FormatsViewProps {
  capabilities: FormatCapability[];
  loading: boolean;
  error: string | null;
  onRetry: () => void;
}

const STATUS_LABEL = {
  supported: "Available",
  readonly: "Read only",
  planned: "Planned",
};

export function FormatsView({
  capabilities,
  loading,
  error,
  onRetry,
}: FormatsViewProps) {
  const readable = capabilities.filter((item) => item.canRead);
  const planned = capabilities.filter((item) => item.status === "planned");
  const zip = capabilities.find((item) => item.format === "zip");

  return (
    <div className="formats-view">
      <header className="view-heading">
        <span className="eyebrow">Engine registry</span>
        <h1>Honest format capabilities</h1>
        <p>
          Archon reports what the installed engine can actually do. File extensions are not
          promises: the capability matrix below is read from the Rust engine at runtime.
        </p>
      </header>

      <section className="phase-banner">
        <span className="phase-banner-icon">
          <Layers3 size={19} />
        </span>
        <div>
          <strong>ZIP is writable; 7Z and TAR are read-only.</strong>
          <p>
            Create, update, extract and test ZIP archives. Browse, extract and test 7Z plus
            the TAR/GZ/BZ2/XZ/ZST family. Formats without an engine stay visible and are
            rejected with a concrete reason.
          </p>
        </div>
        <span className="phase-banner-count">
          {readable.length} of {capabilities.length || "—"} readable
        </span>
      </section>

      {error ? (
        <div className="inline-error" role="alert">
          <span>{error}</span>
          <button type="button" className="btn btn-ghost btn-small" onClick={onRetry}>
            <RefreshCw size={14} />
            Retry
          </button>
        </div>
      ) : null}

      <section className="capability-panel">
        <header className="capability-heading">
          <div>
            <span className="eyebrow">Capability matrix</span>
            <h2>What this build can do</h2>
          </div>
          <span className="capability-legend">
            <span className="status-dot" />
            Runtime registry
          </span>
        </header>

        {loading ? (
          <div className="capability-loading">
            <span className="progress-spinner" />
            Reading engine capabilities…
          </div>
        ) : (
          <div className="capability-scroll">
            <div className="capability-table" role="table" aria-label="Format capabilities">
              <div className="capability-row capability-head" role="row">
                <div role="columnheader">Format</div>
                <div role="columnheader">Status</div>
                <div role="columnheader">Read</div>
                <div role="columnheader">Create</div>
                <div role="columnheader">Modify</div>
                <div role="columnheader">Test</div>
                <div role="columnheader">AES</div>
                <div role="columnheader">Split</div>
              </div>
              {capabilities.map((item) => (
                <div
                  className={`capability-row ${item.status === "supported" ? "is-supported" : ""}`}
                  key={item.format}
                  role="row"
                >
                  <div className="cap-format" role="cell">
                    <span className="cap-format-icon">
                      <FileArchive size={16} />
                    </span>
                    <div>
                      <strong>{item.label}</strong>
                      <span>
                        {item.extensions.length > 0
                          ? item.extensions.map((extension) => `.${extension}`).join(" ")
                          : "No extension"}
                      </span>
                    </div>
                  </div>
                  <div role="cell">
                    <span className={`status-chip status-${item.status}`}>
                      {STATUS_LABEL[item.status]}
                    </span>
                  </div>
                  <CapabilityMark value={item.canRead} />
                  <CapabilityMark value={item.canCreate} />
                  <CapabilityMark value={item.canModify} />
                  <CapabilityMark value={item.canTest} />
                  <CapabilityMark value={item.encryption} />
                  <CapabilityMark value={item.multiVolume} />
                </div>
              ))}
            </div>
          </div>
        )}

        {!loading && capabilities.length > 0 ? (
          <div className="capability-notes">
            {capabilities.map((item) => (
              <details className="cap-note" key={item.format}>
                <summary>
                  <strong>{item.label}</strong>
                  <span>{item.notes}</span>
                </summary>
                <p>{item.notes}</p>
              </details>
            ))}
          </div>
        ) : null}
      </section>

      <div className="formats-grid">
        <section className="format-focus">
          <div className="format-focus-top">
            <span className="format-focus-icon">
              <FileArchive size={25} />
            </span>
            <div>
              <span className="eyebrow">Available now</span>
              <h2>ZIP engine</h2>
            </div>
            <span className="status-chip status-supported">Supported</span>
          </div>

          <p>
            {zip?.notes ??
              "ZIP support is provided by the shared Rust engine. Capabilities are loaded from the backend."}
          </p>

          <div className="feature-grid">
            <div>
              <ShieldCheck size={16} />
              <strong>CRC-32 verification</strong>
              <span>Test reads every file and compares stored checksums.</span>
            </div>
            <div>
              <LockKeyhole size={16} />
              <strong>AES-256 / ZipCrypto</strong>
              <span>Create AE-2 AES-256 archives; decrypt both AES and legacy ZipCrypto.</span>
            </div>
            <div>
              <KeyRound size={16} />
              <strong>In-memory passwords</strong>
              <span>Passwords are never written to logs, settings or recent archives.</span>
            </div>
            <div>
              <AlertTriangle size={16} />
              <strong>Known limits</strong>
              <span>
                Encrypted ZIPs cannot be modified in place; split volumes are application-level,
                not spanned ZIP.
              </span>
            </div>
          </div>
        </section>

        <section className="planned-panel">
          <span className="eyebrow">Roadmap</span>
          <h2>{planned.length} more formats tracked</h2>
          <p>
            Planned formats are visible so the product stays transparent. They are disabled in
            every operation until an engine lands.
          </p>
          <div className="planned-list">
            {planned.map((item) => (
              <span key={item.format}>{item.label}</span>
            ))}
          </div>
          <div className="planned-footnote">
            <Minus size={14} />
            <span>
              No placeholder actions. Unsupported operations return a typed error instead of
              pretending to work.
            </span>
          </div>
        </section>
      </div>
    </div>
  );
}

function CapabilityMark({ value }: { value: boolean }) {
  return (
    <div role="cell" className={`cap-mark ${value ? "is-yes" : "is-no"}`}>
      {value ? <Check size={14} /> : <Minus size={14} />}
      <span className="sr-only">{value ? "Yes" : "No"}</span>
    </div>
  );
}

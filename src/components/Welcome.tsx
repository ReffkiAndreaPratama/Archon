import {
  ArrowRight,
  Check,
  FolderOpen,
  FolderPlus,
  KeyRound,
  LockKeyhole,
  ShieldCheck,
} from "lucide-react";

import { BrandMark } from "./BrandMark";

interface WelcomeProps {
  onOpen: () => void;
  onCreate: () => void;
  dragging: boolean;
}

const FEATURES = [
  {
    icon: ShieldCheck,
    title: "Path-safe by design",
    detail: "Traversal, device names and links are rejected before extraction.",
  },
  {
    icon: LockKeyhole,
    title: "AES-256 prepared",
    detail: "Create and open encrypted ZIP archives without storing passwords.",
  },
  {
    icon: Check,
    title: "Verifiable results",
    detail: "Checksum testing reads every byte and reports real failures.",
  },
];

export function Welcome({ onOpen, onCreate, dragging }: WelcomeProps) {
  return (
    <div className={`welcome ${dragging ? "is-dragging" : ""}`}>
      <section className="welcome-hero">
        <div className="welcome-glow" aria-hidden="true" />
        <div className="welcome-mark">
          <BrandMark size={72} />
        </div>
        <span className="eyebrow">Local archive workspace</span>
        <h1>
          Open it. Pack it.
          <br />
          <em>Know it&apos;s intact.</em>
        </h1>
        <p className="welcome-lede">
          Archon is a focused archive manager built on a shared Rust engine. Create and update
          ZIP archives, then browse, extract and verify 7Z, RAR and TAR — with honest format
          capabilities and no telemetry.
        </p>
        <div className="welcome-actions">
          <button type="button" className="btn btn-primary btn-large" onClick={onOpen}>
            <FolderOpen size={18} />
            Open an archive
          </button>
          <button type="button" className="btn btn-outline btn-large" onClick={onCreate}>
            <FolderPlus size={18} />
            Create a ZIP
            <ArrowRight size={16} />
          </button>
        </div>
        <div className="welcome-chips">
          <span>ZIP read / write</span>
          <span>7Z / RAR / TAR read</span>
          <span>AES-256</span>
          <span>CRC verification</span>
          <span>Path traversal guard</span>
        </div>
      </section>

      <section className="welcome-lower">
        <div className={`drop-card ${dragging ? "is-dragging" : ""}`}>
          <div className="drop-visual" aria-hidden="true">
            <span className="drop-arch drop-arch-outer" />
            <span className="drop-arch drop-arch-inner" />
            <FolderOpen size={27} />
          </div>
          <div>
            <h2>{dragging ? "Release to open" : "Drop an archive here"}</h2>
            <p>
              Drag an archive into the window, or drop any file or folder while in Create to
              add it as a source.
            </p>
          </div>
          <button type="button" className="btn btn-ghost" onClick={onOpen}>
            Browse files
          </button>
        </div>

        <div className="trust-card">
          <div className="trust-heading">
            <span className="trust-icon">
              <KeyRound size={18} />
            </span>
            <div>
              <span className="eyebrow">Privacy</span>
              <h2>Nothing leaves this machine.</h2>
            </div>
          </div>
          <ul className="trust-list">
            {FEATURES.map((feature) => {
              const Icon = feature.icon;
              return (
                <li key={feature.title}>
                  <Icon size={15} />
                  <div>
                    <strong>{feature.title}</strong>
                    <span>{feature.detail}</span>
                  </div>
                </li>
              );
            })}
          </ul>
        </div>
      </section>

      <footer className="welcome-footer">
        <span className="engine-state">
          <span className="status-dot" />
          Archive core ready
        </span>
        <span>ZIP is writable; 7Z, RAR and TAR-family archives are read-only in this build.</span>
      </footer>
    </div>
  );
}

import { AlertTriangle, CheckCircle2, ShieldAlert, ShieldCheck } from "lucide-react";

import { formatBytes, formatDuration, pluralize } from "../lib/format";
import type { TestReport } from "../types/archive";
import { Modal } from "./Modal";

interface TestResultDialogProps {
  report: TestReport;
  archiveName: string;
  onClose: () => void;
}

export function TestResultDialog({ report, archiveName, onClose }: TestResultDialogProps) {
  return (
    <Modal
      title={report.ok ? "Integrity verified" : "Integrity check failed"}
      description={
        report.ok
          ? `${archiveName} passed a full read and checksum test.`
          : `${report.issues.length} ${pluralize(report.issues.length, "issue")} found in ${archiveName}.`
      }
      icon={
        report.ok ? (
          <ShieldCheck size={19} className="tone-success" />
        ) : (
          <ShieldAlert size={19} className="tone-danger" />
        )
      }
      onClose={onClose}
      width="medium"
      footer={
        <button type="button" className="btn btn-primary" onClick={onClose}>
          Close
        </button>
      }
    >
      <div className="result-summary">
        <div className="result-status">
          {report.ok ? <CheckCircle2 size={20} /> : <AlertTriangle size={20} />}
          <div>
            <strong>{report.ok ? "No corruption detected" : "Archive needs attention"}</strong>
            <span>
              {report.entriesChecked} {pluralize(report.entriesChecked, "file")} checked in{" "}
              {formatDuration(report.elapsedMs)}
            </span>
          </div>
        </div>
        <div className="result-grid">
          <div>
            <span>Data read</span>
            <strong>{formatBytes(report.bytesChecked)}</strong>
          </div>
          <div>
            <span>Encrypted</span>
            <strong>
              {report.encryptedEntries} {pluralize(report.encryptedEntries, "entry", "entries")}
            </strong>
          </div>
          <div>
            <span>Issues</span>
            <strong className={report.issues.length > 0 ? "tone-danger" : undefined}>
              {report.issues.length}
            </strong>
          </div>
        </div>
      </div>

      {report.issues.length > 0 ? (
        <div className="issue-list">
          {report.issues.map((issue, index) => (
            <div className="issue-row" key={`${issue.entry}-${index}`}>
              <span className="issue-kind">{issue.kind}</span>
              <div>
                <strong>{issue.entry || "Archive structure"}</strong>
                <p>{issue.message}</p>
              </div>
            </div>
          ))}
        </div>
      ) : null}

      {report.warnings.length > 0 ? (
        <div className="warning-list">
          {report.warnings.map((warning) => (
            <p key={warning}>{warning}</p>
          ))}
        </div>
      ) : null}
    </Modal>
  );
}

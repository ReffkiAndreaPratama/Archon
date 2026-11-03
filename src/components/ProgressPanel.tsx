import { X } from "lucide-react";

import {
  clamp,
  formatBytes,
  humanPhase,
  shortPath,
} from "../lib/format";
import type { ProgressEvent } from "../types/archive";
import type { OperationState } from "../types/ui";

interface ProgressPanelProps {
  operation: OperationState;
  progress: ProgressEvent | null;
  onCancel: () => void;
}

export function ProgressPanel({ operation, progress, onCancel }: OperationProgressProps) {
  const percent = progress
    ? progress.bytesTotal > 0
      ? clamp((progress.bytesDone / progress.bytesTotal) * 100, 0, 100)
      : progress.total > 0
        ? clamp((progress.current / progress.total) * 100, 0, 100)
        : 0
    : 0;
  const hasScale = Boolean(
    progress && (progress.bytesTotal > 0 || progress.total > 0),
  );

  return (
    <section className="progress-panel" aria-live="polite">
      <div className="progress-topline">
        <span className="progress-spinner" aria-hidden="true" />
        <div className="progress-heading">
          <strong>{operation.label}</strong>
          <span>
            {progress ? humanPhase(progress.phase) : "Preparing"} ·{" "}
            {progress?.current ?? 0}
            {progress && progress.total > 0 ? ` / ${progress.total}` : ""}
          </span>
        </div>
        <span className="progress-percent">{hasScale ? `${percent.toFixed(0)}%` : "…"}</span>
        <button
          type="button"
          className="icon-button progress-cancel"
          onClick={onCancel}
          aria-label="Cancel operation"
          title="Cancel operation"
        >
          <X size={16} />
        </button>
      </div>
      <div
        className={`progress-track ${hasScale ? "" : "is-indeterminate"}`}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={hasScale ? Math.round(percent) : undefined}
      >
        <span style={hasScale ? { width: `${percent}%` } : undefined} />
      </div>
      <div className="progress-footer">
        <span title={progress?.path ?? undefined}>
          {progress?.path ? shortPath(progress.path, 64) : "Waiting for the engine…"}
        </span>
        <span>
          {progress && progress.bytesTotal > 0
            ? `${formatBytes(progress.bytesDone)} / ${formatBytes(progress.bytesTotal)}`
            : ""}
        </span>
      </div>
    </section>
  );
}

type OperationProgressProps = ProgressPanelProps;

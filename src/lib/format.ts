export function formatBytes(bytes: number, precision = 1): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  const value = bytes / 1024 ** exponent;
  const digits = exponent === 0 ? 0 : value >= 100 ? 0 : precision;
  return `${value.toFixed(digits)} ${units[exponent]}`;
}

export function formatPercent(value: number): string {
  if (!Number.isFinite(value)) return "—";
  return `${Math.max(0, Math.min(100, value)).toFixed(0)}%`;
}

export function formatDateTime(value: string | null): string {
  if (!value) return "—";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(parsed);
}

export function formatDuration(milliseconds: number): string {
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return "—";
  if (milliseconds < 1000) return `${Math.round(milliseconds)} ms`;
  if (milliseconds < 60_000) return `${(milliseconds / 1000).toFixed(2)} s`;
  const minutes = Math.floor(milliseconds / 60_000);
  const seconds = Math.round((milliseconds % 60_000) / 1000);
  return `${minutes}m ${seconds}s`;
}

export function basename(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || trimmed || path;
}

export function dirname(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const index = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  if (index < 0) return "";
  if (index === 2 && /^[A-Za-z]:/.test(trimmed)) return `${trimmed.slice(0, 2)}\\`;
  return trimmed.slice(0, index);
}

export function stem(path: string): string {
  const name = basename(path);
  const index = name.lastIndexOf(".");
  return index > 0 ? name.slice(0, index) : name;
}

export function extension(path: string): string {
  const name = basename(path);
  const index = name.lastIndexOf(".");
  return index > 0 ? name.slice(index + 1).toLowerCase() : "";
}

export function joinPath(directory: string, name: string): string {
  if (!directory) return name;
  const separator = directory.includes("\\") ? "\\" : "/";
  return directory.replace(/[\\/]+$/, "") + separator + name;
}

export function defaultExtractPath(archivePath: string): string {
  const directory = dirname(archivePath);
  return joinPath(directory || ".", stem(archivePath));
}

export function defaultArchivePath(sourcePath: string): string {
  return joinPath(dirname(sourcePath) || ".", `${stem(sourcePath)}.zip`);
}

export function shortPath(path: string, max = 54): string {
  if (path.length <= max) return path;
  const head = Math.max(8, Math.floor((max - 1) * 0.34));
  const tail = Math.max(12, max - head - 1);
  return `${path.slice(0, head)}…${path.slice(-tail)}`;
}

export function crcHex(value: number): string {
  return value.toString(16).toUpperCase().padStart(8, "0");
}

export function humanPhase(phase: string): string {
  switch (phase) {
    case "scanning":
      return "Scanning sources";
    case "compressing":
      return "Compressing";
    case "extracting":
      return "Extracting";
    case "testing":
      return "Verifying";
    case "finalizing":
      return "Finalising";
    default:
      return phase;
  }
}

export function pluralize(count: number, singular: string, plural?: string): string {
  return count === 1 ? singular : (plural ?? `${singular}s`);
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function cx(...values: Array<string | false | null | undefined>): string {
  return values.filter(Boolean).join(" ");
}

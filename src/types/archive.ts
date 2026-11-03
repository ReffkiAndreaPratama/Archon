export type ArchiveFormat =
  | "zip"
  | "7z"
  | "rar"
  | "rar5"
  | "tar"
  | "tar.gz"
  | "tar.bz2"
  | "tar.xz"
  | "gz"
  | "bz2"
  | "xz"
  | "zst"
  | "wim"
  | "iso"
  | "unknown";

export type CapabilityStatus = "supported" | "readonly" | "planned";

export interface FormatCapability {
  format: ArchiveFormat;
  label: string;
  extensions: string[];
  canRead: boolean;
  canCreate: boolean;
  canModify: boolean;
  canTest: boolean;
  encryption: boolean;
  multiVolume: boolean;
  status: CapabilityStatus;
  notes: string;
}

export interface ArchiveEntry {
  index: number;
  path: string;
  name: string;
  parent: string;
  isDir: boolean;
  isSymlink: boolean;
  size: number;
  compressedSize: number;
  ratio: number;
  modified: string | null;
  encrypted: boolean;
  crc32: number;
  method: string;
}

export interface ArchiveListing {
  path: string;
  format: ArchiveFormat;
  formatLabel: string;
  fileSize: number;
  entries: ArchiveEntry[];
  totalSize: number;
  totalCompressed: number;
  encryptedEntries: number;
  comment: string | null;
  warnings: string[];
}

export interface ArchiveInfo {
  path: string;
  format: ArchiveFormat;
  formatLabel: string;
  archiveSize: number;
  entryCount: number;
  fileCount: number;
  dirCount: number;
  totalSize: number;
  totalCompressed: number;
  ratio: number;
  encryptedEntries: number;
  comment: string | null;
  warnings: string[];
}

export type CompressionPreset = "store" | "fast" | "normal" | "maximum" | "ultra";
export type ZipMethod = "store" | "deflate";
export type OverwritePolicy = "overwrite" | "skip" | "rename";

export interface CreateOptions {
  sources: string[];
  output: string;
  format: "zip";
  preset: CompressionPreset;
  method?: ZipMethod;
  level?: number;
  password?: string;
  exclude: string[];
  overwriteOutput: boolean;
}

export interface CreateReport {
  output: string;
  format: ArchiveFormat;
  entryCount: number;
  fileCount: number;
  dirCount: number;
  totalSize: number;
  archiveSize: number;
  ratio: number;
  elapsedMs: number;
  warnings: string[];
}

export interface ExtractOptions {
  archive: string;
  destination: string;
  entries?: string[];
  password?: string;
  overwrite: OverwritePolicy;
  preserveStructure: boolean;
}

export interface ExtractReport {
  destination: string;
  extractedFiles: number;
  extractedDirs: number;
  skipped: number;
  totalBytes: number;
  elapsedMs: number;
  warnings: string[];
}

export interface TestIssue {
  entry: string;
  kind: string;
  message: string;
}

export interface TestReport {
  ok: boolean;
  entriesChecked: number;
  bytesChecked: number;
  encryptedEntries: number;
  issues: TestIssue[];
  elapsedMs: number;
  warnings: string[];
}

export type ProgressPhase =
  | "scanning"
  | "compressing"
  | "extracting"
  | "testing"
  | "finalizing";

export interface ProgressEvent {
  jobId: string;
  phase: ProgressPhase;
  current: number;
  total: number;
  path: string | null;
  bytesDone: number;
  bytesTotal: number;
}

export interface CommandError {
  code: string;
  message: string;
}

export interface RecentArchive {
  path: string;
  name: string;
  openedAt: number;
  format?: string;
}

export type OperationKind = "open" | "create" | "extract" | "test";

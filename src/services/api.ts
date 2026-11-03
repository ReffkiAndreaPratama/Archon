import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  ArchiveInfo,
  ArchiveListing,
  CommandError,
  CreateOptions,
  CreateReport,
  ExtractOptions,
  ExtractReport,
  FormatCapability,
  ProgressEvent,
  TestReport,
} from "../types/archive";

const PROGRESS_EVENT = "archon://progress";

export const api = {
  capabilities(): Promise<FormatCapability[]> {
    return invoke<FormatCapability[]>("format_capabilities");
  },

  list(path: string): Promise<ArchiveListing> {
    return invoke<ArchiveListing>("list_archive", { path });
  },

  info(path: string): Promise<ArchiveInfo> {
    return invoke<ArchiveInfo>("archive_info", { path });
  },

  create(jobId: string, options: CreateOptions): Promise<CreateReport> {
    return invoke<CreateReport>("create_archive", { jobId, options });
  },

  extract(jobId: string, options: ExtractOptions): Promise<ExtractReport> {
    return invoke<ExtractReport>("extract_archive", { jobId, options });
  },

  test(jobId: string, path: string, password?: string): Promise<TestReport> {
    return invoke<TestReport>("test_archive", { jobId, path, password });
  },

  cancel(jobId: string): Promise<boolean> {
    return invoke<boolean>("cancel_job", { jobId });
  },

  onProgress(handler: (event: ProgressEvent) => void): Promise<UnlistenFn> {
    return listen<ProgressEvent>(PROGRESS_EVENT, (event) => handler(event.payload));
  },
};

export function toCommandError(error: unknown): CommandError {
  if (typeof error === "string") {
    return { code: "internal", message: error };
  }
  if (error && typeof error === "object") {
    const candidate = error as Record<string, unknown>;
    if (typeof candidate.message === "string") {
      return {
        code:
          typeof candidate.code === "string" ? candidate.code : "internal",
        message: candidate.message,
      };
    }
  }
  return {
    code: "internal",
    message: error instanceof Error ? error.message : "An unexpected error occurred.",
  };
}

export function isPasswordError(error: CommandError): boolean {
  return error.code === "password_required" || error.code === "invalid_password";
}

export function isCancellation(error: CommandError): boolean {
  return error.code === "cancelled";
}

export function newJobId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `job-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

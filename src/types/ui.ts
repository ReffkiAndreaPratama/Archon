import type { OperationKind, OverwritePolicy } from "./archive";
import type { ReactNode } from "react";

export interface ToastMessage {
  id: string;
  tone: "success" | "error" | "info";
  title: string;
  detail?: string;
  actionLabel?: string;
  action?: () => void;
}

export interface OperationState {
  kind: OperationKind;
  jobId: string;
  label: string;
}

export interface PasswordPromptState {
  title: string;
  message: string;
  confirmLabel: string;
  invalid: boolean;
}

export interface ExtractFormValues {
  destination: string;
  entries?: string[];
  password?: string;
  overwrite: OverwritePolicy;
  preserveStructure: boolean;
}

export interface CreateSource {
  path: string;
  isDir: boolean;
}

export interface CreateDraft {
  sources: CreateSource[];
  output: string;
  preset: "store" | "fast" | "normal" | "maximum" | "ultra";
  method: "auto" | "store" | "deflate";
  level: number;
  encrypt: boolean;
  password: string;
  confirmPassword: string;
  exclude: string[];
  overwriteOutput: boolean;
}

export interface TableColumn {
  key: string;
  label: string;
  align?: "left" | "right";
  icon?: ReactNode;
}

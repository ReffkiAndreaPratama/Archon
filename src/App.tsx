import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  FolderOpen,
  HardDrive,
  LockKeyhole,
  ShieldCheck,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { ArchiveView } from "./components/ArchiveView";
import { CreateView } from "./components/CreateView";
import { ExtractDialog } from "./components/ExtractDialog";
import { FormatsView } from "./components/FormatsView";
import { PasswordDialog } from "./components/PasswordDialog";
import { ProgressPanel } from "./components/ProgressPanel";
import { Sidebar, type AppView } from "./components/Sidebar";
import { TestResultDialog } from "./components/TestResultDialog";
import { Toasts } from "./components/Toasts";
import { Welcome } from "./components/Welcome";
import {
  basename,
  cx,
  defaultArchivePath,
  formatBytes,
  formatPercent,
  pluralize,
} from "./lib/format";
import {
  api,
  isCancellation,
  isPasswordError,
  newJobId,
  toCommandError,
} from "./services/api";
import type {
  ArchiveListing,
  CreateReport,
  ExtractReport,
  FormatCapability,
  ProgressEvent,
  RecentArchive,
  TestReport,
} from "./types/archive";
import type {
  CreateDraft,
  CreateSource,
  ExtractFormValues,
  OperationState,
  PasswordPromptState,
  ToastMessage,
} from "./types/ui";

const RECENT_KEY = "archon.recent.v1";
const ARCHIVE_SUFFIXES = [
  ".zip",
  ".7z",
  ".rar",
  ".tar",
  ".tar.gz",
  ".tar.bz2",
  ".tar.xz",
  ".tgz",
  ".tbz2",
  ".txz",
  ".gz",
  ".bz2",
  ".xz",
  ".zst",
  ".wim",
  ".iso",
];

function emptyCreateDraft(): CreateDraft {
  return {
    sources: [],
    output: "",
    preset: "normal",
    method: "auto",
    level: 6,
    encrypt: false,
    password: "",
    confirmPassword: "",
    exclude: [],
    overwriteOutput: false,
  };
}

function loadRecent(): RecentArchive[] {
  try {
    const raw = window.localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(
        (item): item is Record<string, unknown> =>
          Boolean(item) && typeof item === "object" && typeof item.path === "string",
      )
      .map((item) => ({
        path: item.path as string,
        name:
          typeof item.name === "string" ? item.name : basename(item.path as string),
        openedAt: typeof item.openedAt === "number" ? item.openedAt : Date.now(),
        format: typeof item.format === "string" ? item.format : undefined,
      }))
      .slice(0, 8);
  } catch {
    return [];
  }
}

function looksLikeArchive(path: string): boolean {
  const lower = path.toLocaleLowerCase();
  return ARCHIVE_SUFFIXES.some((suffix) => lower.endsWith(suffix));
}

function guessSourceIsDir(path: string): boolean {
  return !basename(path).includes(".");
}

function normalizeZipOutput(path: string): string {
  const trimmed = path.trim();
  return /\.zip$/i.test(trimmed) ? trimmed : `${trimmed}.zip`;
}

function mergeSources(
  existing: CreateSource[],
  paths: string[],
  directory: boolean,
): CreateSource[] {
  const seen = new Set(existing.map((source) => source.path.toLocaleLowerCase()));
  const additions: CreateSource[] = [];
  for (const path of paths) {
    const key = path.toLocaleLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    additions.push({
      path,
      isDir: directory || guessSourceIsDir(path),
    });
  }
  return [...existing, ...additions];
}

export default function App() {
  const [view, setView] = useState<AppView>("archive");
  const [listing, setListing] = useState<ArchiveListing | null>(null);
  const [recent, setRecent] = useState<RecentArchive[]>(loadRecent);
  const [capabilities, setCapabilities] = useState<FormatCapability[]>([]);
  const [capabilityLoading, setCapabilityLoading] = useState(true);
  const [capabilityError, setCapabilityError] = useState<string | null>(null);
  const [operation, setOperation] = useState<OperationState | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [opening, setOpening] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const [passwordPrompt, setPasswordPrompt] = useState<PasswordPromptState | null>(null);
  const [extractState, setExtractState] = useState<{ selection: string[] } | null>(null);
  const [testResult, setTestResult] = useState<TestReport | null>(null);
  const [createDraft, setCreateDraft] = useState<CreateDraft>(emptyCreateDraft);

  const operationRef = useRef<OperationState | null>(null);
  const passwordResolver = useRef<((value: string | null) => void) | null>(null);
  const sessionPasswords = useRef(new Map<string, string>());
  const toastTimers = useRef(new Map<string, number>());
  const dropHandler = useRef<(paths: string[]) => void>(() => undefined);

  const busy = opening || operation !== null;

  useEffect(() => {
    operationRef.current = operation;
  }, [operation]);

  useEffect(() => {
    try {
      window.localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
    } catch {
      // Recent archives are a convenience; storage may be unavailable.
    }
  }, [recent]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;

    void api
      .onProgress((event) => {
        if (operationRef.current?.jobId === event.jobId) {
          setProgress(event);
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // Progress is supplemental; operations still return their result.
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    for (const timer of toastTimers.current.values()) {
      window.clearTimeout(timer);
    }
    return () => {
      for (const timer of toastTimers.current.values()) {
        window.clearTimeout(timer);
      }
    };
  }, []);

  const dismissToast = useCallback((id: string) => {
    const timer = toastTimers.current.get(id);
    if (timer) {
      window.clearTimeout(timer);
      toastTimers.current.delete(id);
    }
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const pushToast = useCallback(
    (toast: Omit<ToastMessage, "id">) => {
      const id = newJobId();
      setToasts((current) => [...current.slice(-3), { ...toast, id }]);
      const timer = window.setTimeout(
        () => dismissToast(id),
        toast.tone === "error" ? 10_000 : 6_500,
      );
      toastTimers.current.set(id, timer);
    },
    [dismissToast],
  );

  const notifyError = useCallback(
    (error: unknown, title: string) => {
      const commandError = toCommandError(error);
      if (isCancellation(commandError)) {
        pushToast({
          tone: "info",
          title: "Operation cancelled",
          detail: "No further changes were written.",
        });
        return;
      }
      pushToast({
        tone: "error",
        title,
        detail: commandError.message,
      });
    },
    [pushToast],
  );

  const rememberArchive = useCallback((path: string, format?: string) => {
    setRecent((current) => [
      {
        path,
        name: basename(path),
        format,
        openedAt: Date.now(),
      },
      ...current.filter(
        (item) => item.path.toLocaleLowerCase() !== path.toLocaleLowerCase(),
      ),
    ].slice(0, 8));
  }, []);

  const loadCapabilities = useCallback(async () => {
    setCapabilityLoading(true);
    setCapabilityError(null);
    try {
      setCapabilities(await api.capabilities());
    } catch (error) {
      setCapabilityError(toCommandError(error).message);
    } finally {
      setCapabilityLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadCapabilities();
  }, [loadCapabilities]);

  const openArchive = useCallback(
    async (path: string, quiet = false) => {
      if (operationRef.current || opening) return;
      setOpening(true);
      try {
        const next = await api.list(path);
        if (listing && listing.path !== next.path) {
          sessionPasswords.current.delete(listing.path);
        }
        setListing(next);
        setView("archive");
        rememberArchive(next.path, next.formatLabel);
        if (!quiet) {
          pushToast({
            tone: "info",
            title: `Opened ${basename(next.path)}`,
            detail: `${next.entries.length} ${pluralize(next.entries.length, "entry", "entries")} · ${formatBytes(next.fileSize)}`,
          });
        }
      } catch (error) {
        notifyError(error, "Could not open archive");
      } finally {
        setOpening(false);
      }
    },
    [listing, notifyError, opening, pushToast, rememberArchive],
  );

  const chooseArchive = useCallback(async () => {
    if (operationRef.current || opening) return;
    try {
      const selected = await openDialog({
        title: "Open archive",
        multiple: false,
        directory: false,
        filters: [
          { name: "ZIP archive", extensions: ["zip"] },
          {
            name: "Recognised archive formats",
            extensions: [
              "zip",
              "7z",
              "rar",
              "tar",
              "gz",
              "tgz",
              "bz2",
              "xz",
              "zst",
              "wim",
              "iso",
            ],
          },
        ],
      });
      if (typeof selected === "string") {
        await openArchive(selected);
      }
    } catch (error) {
      notifyError(error, "The file dialog could not be opened");
    }
  }, [notifyError, openArchive, opening]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      const key = event.key.toLocaleLowerCase();
      if (key === "o") {
        event.preventDefault();
        if (!passwordPrompt && !extractState) void chooseArchive();
      } else if (key === "n") {
        event.preventDefault();
        setView("create");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [chooseArchive, extractState, passwordPrompt]);

  const addSources = useCallback((paths: string[], directory = false) => {
    if (paths.length === 0) return;
    setCreateDraft((current) => {
      const sources = mergeSources(current.sources, paths, directory);
      if (sources.length === current.sources.length) return current;
      return {
        ...current,
        sources,
        output:
          current.output.trim() || sources.length === 0
            ? current.output
            : defaultArchivePath(sources[0].path),
      };
    });
  }, []);

  const addFiles = useCallback(async () => {
    try {
      const selected = await openDialog({
        title: "Add files to archive",
        multiple: true,
        directory: false,
      });
      const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
      addSources(paths, false);
    } catch (error) {
      notifyError(error, "The file dialog could not be opened");
    }
  }, [addSources, notifyError]);

  const addFolders = useCallback(async () => {
    try {
      const selected = await openDialog({
        title: "Add folders to archive",
        multiple: true,
        directory: true,
        recursive: true,
      });
      const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
      addSources(paths, true);
    } catch (error) {
      notifyError(error, "The folder dialog could not be opened");
    }
  }, [addSources, notifyError]);

  const removeSource = useCallback((path: string) => {
    setCreateDraft((current) => ({
      ...current,
      sources: current.sources.filter((source) => source.path !== path),
    }));
  }, []);

  const requestPassword = useCallback(
    (request: PasswordPromptState): Promise<string | null> =>
      new Promise((resolve) => {
        if (passwordResolver.current) passwordResolver.current(null);
        passwordResolver.current = resolve;
        setPasswordPrompt(request);
      }),
    [],
  );

  const finishPassword = (value: string | null) => {
    const resolve = passwordResolver.current;
    passwordResolver.current = null;
    setPasswordPrompt(null);
    resolve?.(value);
  };

  const runJob = async <T,>(
    kind: OperationState["kind"],
    label: string,
    work: (jobId: string) => Promise<T>,
  ): Promise<T> => {
    const jobId = newJobId();
    setOperation({ kind, jobId, label });
    setProgress(null);
    try {
      return await work(jobId);
    } finally {
      setOperation((current) => (current?.jobId === jobId ? null : current));
      setProgress(null);
    }
  };

  const cancelOperation = async () => {
    if (!operation) return;
    try {
      const accepted = await api.cancel(operation.jobId);
      pushToast({
        tone: "info",
        title: accepted ? "Cancellation requested" : "Nothing to cancel",
        detail: accepted
          ? "The engine will stop at its next checkpoint."
          : "The operation may already have finished.",
      });
    } catch (error) {
      notifyError(error, "Could not cancel the operation");
    }
  };

  const performExtract = async (values: ExtractFormValues) => {
    if (!listing) return;
    let password =
      values.password || sessionPasswords.current.get(listing.path) || undefined;

    for (let attempt = 0; attempt < 4; attempt += 1) {
      try {
        const report = await runJob<ExtractReport>(
          "extract",
          "Extracting archive",
          (jobId) =>
            api.extract(jobId, {
              archive: listing.path,
              destination: values.destination,
              entries: values.entries,
              password,
              overwrite: values.overwrite,
              preserveStructure: values.preserveStructure,
            }),
        );
        if (password) sessionPasswords.current.set(listing.path, password);
        pushToast({
          tone: "success",
          title: "Extraction complete",
          detail: `${report.extractedFiles} ${pluralize(report.extractedFiles, "file")} · ${formatBytes(report.totalBytes)}${report.skipped > 0 ? ` · ${report.skipped} skipped` : ""}`,
          actionLabel: "Open folder",
          action: () => {
            void revealItemInDir(report.destination).catch(() => undefined);
          },
        });
        return;
      } catch (error) {
        const commandError = toCommandError(error);
        if (isPasswordError(commandError) && attempt < 3) {
          const entered = await requestPassword({
            title:
              commandError.code === "invalid_password"
                ? "Incorrect password"
                : "Password required",
            message:
              commandError.code === "invalid_password"
                ? `The password did not unlock ${basename(listing.path)}. Try again.`
                : `${basename(listing.path)} contains encrypted entries. Enter its password to extract.`,
            confirmLabel: "Extract",
            invalid: commandError.code === "invalid_password",
          });
          if (entered === null) return;
          password = entered;
          continue;
        }
        notifyError(error, "Extraction failed");
        return;
      }
    }

    pushToast({
      tone: "error",
      title: "Could not unlock archive",
      detail: "Password attempts exhausted.",
    });
  };

  const performTest = async () => {
    if (!listing) return;
    let password = sessionPasswords.current.get(listing.path) || undefined;

    for (let attempt = 0; attempt < 4; attempt += 1) {
      try {
        const report = await runJob<TestReport>("test", "Verifying archive", (jobId) =>
          api.test(jobId, listing.path, password),
        );
        if (password) sessionPasswords.current.set(listing.path, password);
        setTestResult(report);
        return;
      } catch (error) {
        const commandError = toCommandError(error);
        if (isPasswordError(commandError) && attempt < 3) {
          const entered = await requestPassword({
            title:
              commandError.code === "invalid_password"
                ? "Incorrect password"
                : "Password required",
            message:
              commandError.code === "invalid_password"
                ? `The password did not unlock ${basename(listing.path)}. Try again.`
                : `${basename(listing.path)} contains encrypted entries. Enter its password to verify it.`,
            confirmLabel: "Verify",
            invalid: commandError.code === "invalid_password",
          });
          if (entered === null) return;
          password = entered;
          continue;
        }
        notifyError(error, "Integrity test failed");
        return;
      }
    }

    pushToast({
      tone: "error",
      title: "Could not unlock archive",
      detail: "Password attempts exhausted.",
    });
  };

  const performCreate = async () => {
    if (createDraft.sources.length === 0) return;
    const output = normalizeZipOutput(createDraft.output);
    if (output !== createDraft.output) {
      setCreateDraft((current) => ({ ...current, output }));
    }

    const method = createDraft.method === "auto" ? undefined : createDraft.method;
    const options = {
      sources: createDraft.sources.map((source) => source.path),
      output,
      format: "zip" as const,
      preset: createDraft.preset,
      method,
      level: method === "deflate" ? createDraft.level : undefined,
      password: createDraft.encrypt ? createDraft.password : undefined,
      exclude: createDraft.exclude,
      overwriteOutput: createDraft.overwriteOutput,
    };

    try {
      const report = await runJob<CreateReport>("create", "Packing archive", (jobId) =>
        api.create(jobId, options),
      );
      setCreateDraft(emptyCreateDraft());
      await openArchive(report.output, true);
      pushToast({
        tone: "success",
        title: "Archive created",
        detail: `${report.fileCount} ${pluralize(report.fileCount, "file")} · ${formatBytes(report.archiveSize)} · ${formatPercent(report.ratio)} saved`,
        actionLabel: "Show in Explorer",
        action: () => {
          void revealItemInDir(report.output).catch(() => undefined);
        },
      });
    } catch (error) {
      notifyError(error, "Could not create archive");
    }
  };

  const handleDropped = (paths: string[]) => {
    if (paths.length === 0) return;
    if (view === "create") {
      addSources(paths, false);
      return;
    }
    if (paths.length === 1 && looksLikeArchive(paths[0])) {
      void openArchive(paths[0]);
      return;
    }
    addSources(paths, false);
    setView("create");
  };
  dropHandler.current = handleDropped;

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter" || payload.type === "over") {
          setDragging(true);
        } else if (payload.type === "leave") {
          setDragging(false);
        } else if (payload.type === "drop") {
          setDragging(false);
          dropHandler.current(payload.paths);
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // Drag and drop is unavailable in non-desktop contexts.
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const handleReveal = () => {
    if (!listing) return;
    void revealItemInDir(listing.path).catch((error: unknown) => {
      notifyError(error, "Could not open Explorer");
    });
  };

  const handleCloseArchive = () => {
    if (listing) sessionPasswords.current.delete(listing.path);
    setListing(null);
    setView("archive");
  };

  const handleOpenRecent = (path: string) => {
    void openArchive(path);
  };

  const removeRecent = (path: string) => {
    setRecent((current) =>
      current.filter(
        (item) => item.path.toLocaleLowerCase() !== path.toLocaleLowerCase(),
      ),
    );
  };

  const archiveMode = view === "archive" && listing !== null;

  return (
    <div className="app-shell">
      <Sidebar
        view={view}
        recent={recent}
        onViewChange={setView}
        onOpen={() => void chooseArchive()}
        onOpenRecent={handleOpenRecent}
        onRemoveRecent={removeRecent}
        onClearRecent={() => setRecent([])}
      />

      <div className="app-main">
        <header className="topbar">
          <div className="topbar-title">
            <span className="eyebrow">Workspace</span>
            <span className="topbar-view">
              {view === "archive" ? "Archive" : view === "create" ? "Create" : "Formats"}
            </span>
          </div>
          <div className="topbar-actions">
            <span className="topbar-pill">
              <LockKeyhole size={13} />
              Local only
            </span>
            <span className="topbar-pill">
              <span className="status-dot" />
              ZIP engine
            </span>
            <button
              type="button"
              className="btn btn-outline btn-small"
              onClick={() => void chooseArchive()}
              disabled={busy}
            >
              <FolderOpen size={15} />
              Open
              <kbd>Ctrl O</kbd>
            </button>
          </div>
        </header>

        <main
          className={cx(
            "app-content",
            archiveMode ? "is-fixed" : "is-scrollable",
          )}
        >
          {view === "archive" &&
            (listing ? (
              <ArchiveView
                key={listing.path}
                listing={listing}
                busy={busy}
                onTest={() => void performTest()}
                onExtract={(selection) => setExtractState({ selection })}
                onClose={handleCloseArchive}
                onReveal={handleReveal}
              />
            ) : (
              <Welcome
                onOpen={() => void chooseArchive()}
                onCreate={() => setView("create")}
                dragging={dragging}
              />
            ))}

          {view === "create" ? (
            <CreateView
              draft={createDraft}
              busy={busy}
              dragging={dragging}
              onChange={(patch) =>
                setCreateDraft((current) => ({ ...current, ...patch }))
              }
              onAddFiles={() => void addFiles()}
              onAddFolders={() => void addFolders()}
              onRemoveSource={removeSource}
              onCreate={() => void performCreate()}
            />
          ) : null}

          {view === "formats" ? (
            <FormatsView
              capabilities={capabilities}
              loading={capabilityLoading}
              error={capabilityError}
              onRetry={() => void loadCapabilities()}
            />
          ) : null}

          {opening ? (
            <div className="content-loading" role="status">
              <span className="progress-spinner" />
              <span>Reading archive…</span>
            </div>
          ) : null}

          {dragging && archiveMode ? (
            <div className="drop-overlay">
              <div>
                <HardDrive size={30} />
                <strong>Drop to open or add sources</strong>
                <span>Archives open here. Other files and folders become Create sources.</span>
              </div>
            </div>
          ) : null}
        </main>

        <footer className="statusbar">
          <span className="statusbar-state">
            <span
              className={cx(
                "status-dot",
                (opening || operation) && "is-busy",
              )}
            />
            {opening ? "Reading archive" : operation ? operation.label : "Ready"}
          </span>
          <span className="statusbar-divider" />
          <span className="statusbar-engine">
            <ShieldCheck size={13} />
            Rust archive core
          </span>
          <span className="statusbar-spacer" />
          {listing ? (
            <span>
              {listing.entries.length} {pluralize(listing.entries.length, "entry", "entries")} ·{" "}
              {formatBytes(listing.fileSize)}
            </span>
          ) : (
            <span>ZIP only in Phase 1</span>
          )}
          <span className="statusbar-divider" />
          <span>No telemetry</span>
        </footer>
      </div>

      {operation ? (
        <ProgressPanel
          operation={operation}
          progress={progress}
          onCancel={() => void cancelOperation()}
        />
      ) : null}

      <Toasts toasts={toasts} onDismiss={dismissToast} />

      {passwordPrompt ? (
        <PasswordDialog
          title={passwordPrompt.title}
          message={passwordPrompt.message}
          confirmLabel={passwordPrompt.confirmLabel}
          invalid={passwordPrompt.invalid}
          onSubmit={finishPassword}
          onCancel={() => finishPassword(null)}
        />
      ) : null}

      {extractState && listing ? (
        <ExtractDialog
          listing={listing}
          selection={extractState.selection}
          initialPassword={sessionPasswords.current.get(listing.path) ?? ""}
          onConfirm={(values) => {
            setExtractState(null);
            void performExtract(values);
          }}
          onClose={() => setExtractState(null)}
        />
      ) : null}

      {testResult && listing ? (
        <TestResultDialog
          report={testResult}
          archiveName={basename(listing.path)}
          onClose={() => setTestResult(null)}
        />
      ) : null}
    </div>
  );
}

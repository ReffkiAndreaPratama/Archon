import {
  File,
  FileArchive,
  FileAudio,
  FileCode2,
  FileImage,
  FileJson,
  FileSpreadsheet,
  FileText,
  FileVideo,
  Folder,
  FolderOpen,
} from "lucide-react";

import { cx, extension } from "../lib/format";

interface FileGlyphProps {
  name: string;
  isDir?: boolean;
  open?: boolean;
  size?: number;
}

const CODE_EXTENSIONS = new Set([
  "js",
  "jsx",
  "ts",
  "tsx",
  "rs",
  "py",
  "go",
  "java",
  "c",
  "h",
  "cpp",
  "cs",
  "rb",
  "php",
  "html",
  "css",
  "scss",
  "sh",
  "ps1",
]);
const ARCHIVE_EXTENSIONS = new Set([
  "zip",
  "7z",
  "rar",
  "tar",
  "gz",
  "tgz",
  "bz2",
  "xz",
  "zst",
]);
const IMAGE_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "bmp",
  "svg",
  "ico",
  "tif",
  "tiff",
  "avif",
]);
const VIDEO_EXTENSIONS = new Set(["mp4", "mkv", "mov", "avi", "webm", "wmv", "m4v"]);
const AUDIO_EXTENSIONS = new Set(["mp3", "wav", "flac", "ogg", "m4a", "aac", "wma"]);
const SHEET_EXTENSIONS = new Set(["csv", "xls", "xlsx", "ods"]);
const TEXT_EXTENSIONS = new Set(["txt", "md", "log", "rtf", "pdf", "doc", "docx"]);

export function FileGlyph({ name, isDir = false, open = false, size = 17 }: FileGlyphProps) {
  if (isDir) {
    const Icon = open ? FolderOpen : Folder;
    return (
      <span className="file-glyph is-folder">
        <Icon size={size} strokeWidth={1.8} />
      </span>
    );
  }

  const ext = extension(name);
  let Icon = File;
  let tone = "is-neutral";

  if (ARCHIVE_EXTENSIONS.has(ext)) {
    Icon = FileArchive;
    tone = "is-archive";
  } else if (IMAGE_EXTENSIONS.has(ext)) {
    Icon = FileImage;
    tone = "is-image";
  } else if (VIDEO_EXTENSIONS.has(ext)) {
    Icon = FileVideo;
    tone = "is-video";
  } else if (AUDIO_EXTENSIONS.has(ext)) {
    Icon = FileAudio;
    tone = "is-audio";
  } else if (CODE_EXTENSIONS.has(ext)) {
    Icon = FileCode2;
    tone = "is-code";
  } else if (SHEET_EXTENSIONS.has(ext)) {
    Icon = FileSpreadsheet;
    tone = "is-sheet";
  } else if (ext === "json" || ext === "jsonl" || ext === "xml" || ext === "yaml" || ext === "yml") {
    Icon = FileJson;
    tone = "is-code";
  } else if (TEXT_EXTENSIONS.has(ext)) {
    Icon = FileText;
    tone = "is-text";
  }

  return (
    <span className={cx("file-glyph", tone)}>
      <Icon size={size} strokeWidth={1.8} />
    </span>
  );
}

import type { ArchiveEntry } from "../types/archive";

export interface TreeNode {
  name: string;
  path: string;
  isDir: boolean;
  entry: ArchiveEntry | null;
  children: Map<string, TreeNode>;
  fileCount: number;
  totalSize: number;
  totalCompressed: number;
}

export interface BrowserRow {
  key: string;
  path: string;
  name: string;
  isDir: boolean;
  size: number;
  compressedSize: number;
  ratio: number;
  modified: string | null;
  encrypted: boolean;
  isSymlink: boolean;
  method: string;
  crc32: number;
  fileCount: number;
  entry: ArchiveEntry | null;
  node: TreeNode;
}

export function buildArchiveTree(entries: ArchiveEntry[]): TreeNode {
  const root: TreeNode = {
    name: "",
    path: "",
    isDir: true,
    entry: null,
    children: new Map(),
    fileCount: 0,
    totalSize: 0,
    totalCompressed: 0,
  };

  for (const entry of entries) {
    const cleanPath = entry.path.replace(/^\/+|\/+$/g, "");
    if (!cleanPath) continue;
    const segments = cleanPath.split("/").filter(Boolean);
    let current = root;

    for (let index = 0; index < segments.length; index += 1) {
      const segment = segments[index];
      const isLeaf = index === segments.length - 1;
      const path = segments.slice(0, index + 1).join("/");
      let child = current.children.get(segment);

      if (!child) {
        child = {
          name: segment,
          path,
          isDir: isLeaf ? entry.isDir : true,
          entry: null,
          children: new Map(),
          fileCount: 0,
          totalSize: 0,
          totalCompressed: 0,
        };
        current.children.set(segment, child);
      }

      if (isLeaf) {
        child.isDir = entry.isDir;
        if (!child.entry || !entry.isDir) child.entry = entry;
      } else {
        child.isDir = true;
      }
      current = child;
    }
  }

  aggregate(root);
  return root;
}

function aggregate(node: TreeNode): void {
  if (!node.isDir) {
    node.fileCount = 1;
    node.totalSize = node.entry?.size ?? 0;
    node.totalCompressed = node.entry?.compressedSize ?? 0;
    return;
  }

  let files = 0;
  let size = 0;
  let compressed = 0;
  for (const child of node.children.values()) {
    aggregate(child);
    files += child.fileCount;
    size += child.totalSize;
    compressed += child.totalCompressed;
  }
  node.fileCount = files;
  node.totalSize = size;
  node.totalCompressed = compressed;
}

export function findNode(root: TreeNode, path: string): TreeNode | null {
  if (!path) return root;
  let current = root;
  for (const segment of path.split("/").filter(Boolean)) {
    const child = current.children.get(segment);
    if (!child) return null;
    current = child;
  }
  return current;
}

export function nodeToRow(node: TreeNode): BrowserRow {
  const entry = node.entry;
  const size = node.isDir ? node.totalSize : (entry?.size ?? 0);
  const compressed = node.isDir ? node.totalCompressed : (entry?.compressedSize ?? 0);
  const ratio =
    size > 0 ? Math.max(0, Math.min(100, ((size - compressed) / size) * 100)) : 0;

  return {
    key: node.path,
    path: node.path,
    name: node.name,
    isDir: node.isDir,
    size,
    compressedSize: compressed,
    ratio,
    modified: entry?.modified ?? null,
    encrypted: entry?.encrypted ?? false,
    isSymlink: entry?.isSymlink ?? false,
    method: entry?.method ?? "",
    crc32: entry?.crc32 ?? 0,
    fileCount: node.fileCount,
    entry,
    node,
  };
}

export function childrenRows(
  root: TreeNode,
  currentPath: string,
): BrowserRow[] {
  const node = findNode(root, currentPath);
  if (!node || !node.isDir) return [];
  return Array.from(node.children.values())
    .sort(compareNodes)
    .map(nodeToRow);
}

export function compareNodes(left: TreeNode, right: TreeNode): number {
  if (left.isDir !== right.isDir) return left.isDir ? -1 : 1;
  return left.name.localeCompare(right.name, undefined, {
    numeric: true,
    sensitivity: "base",
  });
}

export function searchRows(
  root: TreeNode,
  query: string,
  limit = 4000,
): BrowserRow[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return [];

  const matches: BrowserRow[] = [];
  const visit = (node: TreeNode) => {
    if (matches.length >= limit) return;
    if (node.path && node.path.toLocaleLowerCase().includes(needle)) {
      matches.push(nodeToRow(node));
    }
    if (node.isDir) {
      for (const child of node.children.values()) visit(child);
    }
  };
  visit(root);
  return matches.sort((left, right) =>
    left.path.localeCompare(right.path, undefined, {
      numeric: true,
      sensitivity: "base",
    }),
  );
}

export function childFolders(root: TreeNode, currentPath: string): TreeNode[] {
  const node = findNode(root, currentPath);
  if (!node) return [];
  return Array.from(node.children.values())
    .filter((child) => child.isDir)
    .sort(compareNodes);
}

export function breadcrumbSegments(path: string): Array<{ name: string; path: string }> {
  const segments = path.split("/").filter(Boolean);
  return segments.map((name, index) => ({
    name,
    path: segments.slice(0, index + 1).join("/"),
  }));
}

export function sortRows(rows: BrowserRow[], key: SortKey, direction: SortDirection): BrowserRow[] {
  return [...rows].sort((left, right) => {
    if (left.isDir !== right.isDir) return left.isDir ? -1 : 1;

    let comparison = 0;
    switch (key) {
      case "size":
        comparison = left.size - right.size;
        break;
      case "compressed":
        comparison = left.compressedSize - right.compressedSize;
        break;
      case "modified":
        comparison = (left.modified ?? "").localeCompare(right.modified ?? "");
        break;
      case "name":
      default:
        comparison = left.name.localeCompare(right.name, undefined, {
          numeric: true,
          sensitivity: "base",
        });
        break;
    }
    return direction === "asc" ? comparison : -comparison;
  });
}

export type SortKey = "name" | "size" | "compressed" | "modified";
export type SortDirection = "asc" | "desc";

import { vault as vaultApi, indexer as indexerApi } from '../ts/ipc';
import type { TreeEntry, NoteData, WatchEvent } from '../ts/types';

// Vault state
let vaultPath = $state<string | null>(null);
let isOpen = $derived(vaultPath !== null);
let tree = $state<TreeEntry[]>([]);
let currentNotePath = $state<string | null>(null);
let currentNoteData = $state<NoteData | null>(null);
let currentNoteContent = $state<string>('');
let isLoading = $state(false);
let error = $state<string | null>(null);

// File tree helpers
function flattenFiles(entries: TreeEntry[]): string[] {
  const files: string[] = [];
  for (const entry of entries) {
    if (entry.type === 'file') files.push(entry.name);
    else files.push(...flattenFiles(entry.children));
  }
  return files;
}

let allFiles = $derived(flattenFiles(tree));

// Actions
async function openVault(path: string) {
  isLoading = true;
  error = null;
  try {
    await vaultApi.open(path);
    vaultPath = path;
    tree = await vaultApi.getTree();
    currentNotePath = null;
    currentNoteData = null;
    currentNoteContent = '';
  } catch (e) {
    error = e instanceof Error ? e.message : String(e);
    vaultPath = null;
    tree = [];
  } finally {
    isLoading = false;
  }
}

async function closeVault() {
  try {
    await vaultApi.close();
  } catch { /* ignore */ }
  vaultPath = null;
  tree = [];
  currentNotePath = null;
  currentNoteData = null;
  currentNoteContent = '';
  error = null;
}

async function selectNote(path: string) {
  currentNotePath = path;
  try {
    const [content, data] = await Promise.all([
      vaultApi.readNote(path),
      indexerApi.getNote(path),
    ]);
    currentNoteContent = content;
    currentNoteData = data;
  } catch (e) {
    error = e instanceof Error ? e.message : String(e);
  }
}

async function refreshTree() {
  if (!isOpen) return;
  try {
    tree = await vaultApi.getTree();
  } catch { /* ignore */ }
}

function handleWatchEvent(event: WatchEvent) {
  if (event.kind === 'bulk-change') {
    refreshTree();
    return;
  }
  // Refresh tree on any change
  refreshTree();
  // If current note was modified externally, refresh its content
  if (event.path === currentNotePath && event.kind === 'modified') {
    selectNote(event.path);
  }
  // If current note was deleted, clear selection
  if (event.path === currentNotePath && event.kind === 'removed') {
    currentNotePath = null;
    currentNoteData = null;
    currentNoteContent = '';
  }
}

export function getVaultStore() {
  return {
    get vaultPath() { return vaultPath; },
    get isOpen() { return isOpen; },
    get tree() { return tree; },
    get currentNotePath() { return currentNotePath; },
    get currentNoteData() { return currentNoteData; },
    get currentNoteContent() { return currentNoteContent; },
    get isLoading() { return isLoading; },
    get error() { return error; },
    get allFiles() { return allFiles; },
    openVault,
    closeVault,
    selectNote,
    refreshTree,
    handleWatchEvent,
    setCurrentNoteContent(content: string) { currentNoteContent = content; },
    setCurrentNoteData(data: NoteData | null) { currentNoteData = data; },
    clearError() { error = null; },
  };
}

import { invoke } from '@tauri-apps/api/core';
import type {
  TreeEntry, NoteData, Backlink, GraphData,
  SearchResult, WatchEvent
} from './types';

export const vault = {
  open: (path: string) => invoke<void>('open_vault', { path }),
  close: () => invoke<void>('close_vault'),
  getTree: () => invoke<TreeEntry[]>('get_tree'),
  readNote: (path: string) => invoke<string>('read_note', { path }),
  createNote: (path: string, content: string) => invoke<void>('create_note', { path, content }),
  updateNote: (path: string, content: string) => invoke<void>('update_note', { path, content }),
  deleteNote: (path: string) => invoke<void>('delete_note', { path }),
  renameNote: (oldPath: string, newPath: string) => invoke<void>('rename_note', { oldPath, newPath }),
  createFolder: (path: string) => invoke<void>('create_folder', { path }),
};

export const indexer = {
  rebuild: () => invoke<void>('rebuild_index'),
  getNote: (path: string) => invoke<NoteData | null>('get_note', { path }),
  getBacklinks: (path: string) => invoke<Backlink[]>('get_backlinks', { path }),
  getGraphData: () => invoke<GraphData>('get_graph_data'),
  getTags: () => invoke<Record<string, number>>('get_tags'),
  getNotesByTag: (tag: string) => invoke<NoteData[]>('get_notes_by_tag', { tag }),
  resolveWikilink: (raw: string) => invoke<string | null>('resolve_wikilink', { raw }),
};

export const search = {
  search: (query: string, limit?: number) => invoke<SearchResult[]>('search', { query, limit }),
  searchByTag: (tag: string, limit?: number) => invoke<SearchResult[]>('search_by_tag', { tag, limit }),
  suggest: (prefix: string, limit?: number) => invoke<string[]>('suggest', { prefix, limit }),
};

export const watcher = {
  start: () => invoke<void>('start_watcher'),
  stop: () => invoke<void>('stop_watcher'),
};

// Tauri event listener
export { listen } from '@tauri-apps/api/event';
export type { UnlistenFn } from '@tauri-apps/api/event';

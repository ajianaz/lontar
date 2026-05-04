import { vault as vaultApi, indexer } from '../ts/ipc';
import { getVaultStore } from './vault.svelte';
import { AUTO_SAVE_DELAY_MS } from '../ts/constants';

interface Tab {
  path: string;
  title: string;
  isDirty: boolean;
}

let tabs = $state<Tab[]>([]);
let activeTabIndex = $state(-1);
let saveTimeout: ReturnType<typeof setTimeout> | null = null;

const vault = getVaultStore();

let activeTab = $derived(
  activeTabIndex >= 0 && activeTabIndex < tabs.length
    ? tabs[activeTabIndex]
    : null
);

function openTab(path: string) {
  const existingIdx = tabs.findIndex(t => t.path === path);
  if (existingIdx >= 0) {
    activeTabIndex = existingIdx;
    return;
  }
  const title = path.split('/').pop()?.replace('.md', '') || path;
  tabs = [...tabs, { path, title, isDirty: false }];
  activeTabIndex = tabs.length - 1;
  vault.selectNote(path);
}

function closeTab(index: number) {
  const tab = tabs[index];
  if (!tab || tab.isDirty) return; // Don't close dirty tabs without confirmation
  tabs = tabs.filter((_, i) => i !== index);
  if (tabs.length === 0) {
    activeTabIndex = -1;
  } else if (activeTabIndex >= tabs.length) {
    activeTabIndex = tabs.length - 1;
  } else if (index < activeTabIndex) {
    activeTabIndex--;
  }
}

function markDirty() {
  if (activeTab) {
    tabs = tabs.map((t, i) => i === activeTabIndex ? { ...t, isDirty: true } : t);
  }
}

function scheduleSave(content: string) {
  if (saveTimeout) clearTimeout(saveTimeout);
  if (!activeTab) return;
  saveTimeout = setTimeout(async () => {
    if (!activeTab) return;
    try {
      await vaultApi.updateNote(activeTab.path, content);
      tabs = tabs.map((t, i) => i === activeTabIndex ? { ...t, isDirty: false } : t);
    } catch { /* silent — user will see error on next explicit save */ }
    saveTimeout = null;
  }, AUTO_SAVE_DELAY_MS);
}

function flushSave() {
  if (saveTimeout) {
    clearTimeout(saveTimeout);
    saveTimeout = null;
  }
}

export function getEditorStore() {
  return {
    get tabs() { return tabs; },
    get activeTabIndex() { return activeTabIndex; },
    get activeTab() { return activeTab; },
    openTab,
    closeTab,
    markDirty,
    scheduleSave,
    flushSave,
    setActiveTabIndex(i: number) { activeTabIndex = i; },
  };
}

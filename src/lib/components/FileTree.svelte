<script lang="ts">
  import type { TreeEntry } from '../ts/types';
  import { getVaultStore } from '../stores/vault.svelte';
  import { getEditorStore } from '../stores/editor.svelte';
  import { vault as vaultApi } from '../ts/ipc';

  const vault = getVaultStore();
  const editor = getEditorStore();

  let expandedDirs = $state<Set<string>>(new Set());
  let contextMenu = $state<{ x: number; y: number; path: string; isDir: boolean } | null>(null);
  let newItemName = $state('');
  let showNewItemInput = $state<{ parentDir: string; isDir: boolean } | false>(false);

  // Expand all dirs by default
  $effect(() => {
    if (vault.isOpen) {
      expandedDirs = new Set(collectDirs(vault.tree));
    }
  });

  function collectDirs(entries: TreeEntry[]): string[] {
    const dirs: string[] = [];
    for (const e of entries) {
      if (e.type === 'dir') {
        dirs.push(e.name);
        dirs.push(...collectDirs(e.children));
      }
    }
    return dirs;
  }

  function toggleDir(name: string) {
    const next = new Set(expandedDirs);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    expandedDirs = next;
  }

  function selectFile(name: string) {
    editor.openTab(name);
  }

  function handleContextMenu(e: MouseEvent, path: string, isDir: boolean) {
    e.preventDefault();
    contextMenu = { x: e.clientX, y: e.clientY, path, isDir };
  }

  function closeContextMenu() { contextMenu = null; }

  async function handleDelete() {
    if (!contextMenu) return;
    const { path } = contextMenu;
    contextMenu = null;
    if (!confirm(`Delete "${path}"?`)) return;
    try {
      await vaultApi.deleteNote(path);
      vault.refreshTree();
      // Close all tabs matching the deleted path
      for (let i = editor.tabs.length - 1; i >= 0; i--) {
        if (editor.tabs[i].path === path) {
          editor.closeTab(i, true); // force close since file is deleted
        }
      }
    } catch (e) {
      console.error('Failed to delete note:', e);
      alert(e instanceof Error ? e.message : String(e));
    }
  }

  async function handleCreate() {
    if (!contextMenu) return;
    const { path, isDir } = contextMenu;
    const parentDir = isDir ? path : path.substring(0, path.lastIndexOf('/'));
    contextMenu = null;
    showNewItemInput = { parentDir, isDir: false };
    newItemName = '';
  }

  async function submitNewItem() {
    if (!showNewItemInput || !newItemName.trim()) {
      showNewItemInput = false;
      return;
    }
    const name = newItemName.trim().endsWith('.md') ? newItemName.trim() : newItemName.trim() + '.md';
    const fullPath = showNewItemInput.parentDir ? showNewItemInput.parentDir + '/' + name : name;
    try {
      await vaultApi.createNote(fullPath, '');
      vault.refreshTree();
      editor.openTab(fullPath);
    } catch (e) {
      alert(e instanceof Error ? e.message : String(e));
    }
    showNewItemInput = false;
    newItemName = '';
  }

  // Click outside closes context menu
  $effect(() => {
    if (contextMenu) {
      const handler = () => closeContextMenu();
      document.addEventListener('click', handler, { once: true });
      return () => document.removeEventListener('click', handler);
    }
  });
</script>

{#snippet treeItem(entry: TreeEntry, depth: number = 0)}
  {#if entry.type === 'dir'}
    <div class="tree-item dir" style="padding-left: {depth * 16 + 8}px"
         onclick={() => toggleDir(entry.name)}
         oncontextmenu={(e) => handleContextMenu(e, entry.name, true)}>
      <span class="icon">{expandedDirs.has(entry.name) ? '▾' : '▸'}</span>
      <span class="name">{entry.name}</span>
      {#if showNewItemInput && showNewItemInput.parentDir === entry.name}
        <input
          class="new-item-input"
          value={newItemName}
          onchange={submitNewItem}
          onkeydown={(e) => e.key === 'Enter' && submitNewItem() || e.key === 'Escape' && (showNewItemInput = false)}
          autofocus
          placeholder="note name"
        />
      {/if}
    </div>
    {#if expandedDirs.has(entry.name)}
      {#each entry.children as child}
        {@render treeItem(child, depth + 1)}
      {/each}
    {/if}
  {:else}
    <button
      class="tree-item file"
      class:active={vault.currentNotePath === entry.name}
      style="padding-left: {depth * 16 + 24}px"
      onclick={() => selectFile(entry.name)}
      oncontextmenu={(e) => handleContextMenu(e, entry.name, false)}
    >
      <span class="name">{entry.name.replace('.md', '')}</span>
    </button>
  {/if}
{/snippet}

<nav class="file-tree">
  {#each vault.tree as entry}
    {@render treeItem(entry)}
  {/each}
  {#if vault.tree.length === 0 && vault.isOpen}
    <p class="empty">No notes yet</p>
  {/if}
</nav>

{#if contextMenu}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="context-menu" style="left: {contextMenu.x}px; top: {contextMenu.y}px">
    <button onclick={handleCreate}>New note</button>
    <button onclick={handleDelete}>Delete</button>
  </div>
{/if}

<style>
  .file-tree {
    flex: 1;
    overflow-y: auto;
    padding: 4px 8px;
  }
  .tree-item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.85rem;
    color: var(--text-secondary);
    transition: background 0.1s, color 0.1s;
    white-space: nowrap;
    overflow: hidden;
  }
  .tree-item.dir:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tree-item.file {
    width: 100%;
    text-align: left;
    background: none;
    border: none;
  }
  .tree-item.file:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tree-item.file.active { background: var(--bg-active); color: var(--text-primary); font-weight: 500; }
  .icon { font-size: 0.7rem; width: 12px; text-align: center; flex-shrink: 0; }
  .name { overflow: hidden; text-overflow: ellipsis; }
  .empty { color: var(--text-muted); font-size: 0.8rem; padding: 12px; text-align: center; }
  .new-item-input {
    padding: 2px 6px;
    background: var(--bg-tertiary);
    border: 1px solid var(--accent);
    border-radius: 3px;
    color: var(--text-primary);
    font-size: 0.8rem;
    width: 100%;
    margin-left: 4px;
  }
  .context-menu {
    position: fixed;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px;
    z-index: 1000;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
  }
  .context-menu button {
    display: block;
    width: 100%;
    padding: 6px 12px;
    background: none;
    border: none;
    color: var(--text-secondary);
    font-size: 0.8rem;
    text-align: left;
    cursor: pointer;
    border-radius: 4px;
  }
  .context-menu button:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>

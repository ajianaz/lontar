<script lang="ts">
  import { getVaultStore } from '../stores/vault.svelte';
  import { getEditorStore } from '../stores/editor.svelte';
  import { open } from '@tauri-apps/plugin-dialog';

  const vault = getVaultStore();
  const editor = getEditorStore();
  let isLoading = $state(false);

  async function pickVault() {
    if (isLoading) return;
    try {
      isLoading = true;
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === 'string') {
        await vault.openVault(selected);
      }
    } catch (e) {
      console.error('Failed to open vault:', e);
    } finally {
      isLoading = false;
    }
  }

  function handleCloseVault() {
    const dirtyTabs = editor.tabs.filter(t => t.isDirty);
    if (dirtyTabs.length > 0) {
      if (!confirm(`You have ${dirtyTabs.length} unsaved note(s). Close vault anyway?`)) return;
    }
    // Reset editor state before closing to prevent stale tabs leaking
    editor.flushSave();
    for (let i = editor.tabs.length - 1; i >= 0; i--) {
      editor.closeTab(i, true);
    }
    vault.closeVault();
  }
</script>

{#if !vault.isOpen}
  <div class="vault-picker">
    <button class="open-btn" onclick={pickVault} disabled={isLoading}>
      {isLoading ? 'Opening...' : 'Open Vault'}
    </button>
    <p class="hint">Select a folder as your vault</p>
  </div>
{:else}
  <div class="vault-info">
    <span class="vault-name" title={vault.vaultPath}>
      {vault.vaultPath?.split('/').pop() || 'Vault'}
    </span>
    <button class="close-btn" onclick={handleCloseVault} title="Close vault">
      ✕
    </button>
  </div>
{/if}

<style>
  .vault-picker {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 12px;
    padding: 24px;
  }
  .open-btn {
    padding: 12px 24px;
    background: var(--accent);
    color: var(--bg-primary);
    border: none;
    border-radius: 8px;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }
  .open-btn:hover { background: var(--accent-hover); }
  .hint { color: var(--text-muted); font-size: 0.85rem; }
  .vault-info {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 4px;
  }
  .vault-name {
    font-size: 0.8rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 180px;
  }
  .close-btn {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 0.8rem;
    padding: 2px 6px;
    border-radius: 4px;
    transition: color 0.15s, background 0.15s;
  }
  .close-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
</style>

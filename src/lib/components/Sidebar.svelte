<script lang="ts">
  import { getVaultStore } from '../stores/vault.svelte';
  import { getUiStore } from '../stores/ui.svelte';
  import VaultPicker from './VaultPicker.svelte';
  import SearchBar from './SearchBar.svelte';
  import FileTree from './FileTree.svelte';

  const vault = getVaultStore();
  const ui = getUiStore();
</script>

{#if ui.sidebarVisible}
  <aside class="sidebar" style="width: {ui.sidebarWidth}px">
    <div class="sidebar-header">
      <VaultPicker />
    </div>
    {#if vault.isOpen}
      <div class="search-section">
        <SearchBar />
      </div>
      <FileTree />
    {/if}
  </aside>
{/if}

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border);
    flex-shrink: 0;
    overflow: hidden;
  }
  .sidebar-header {
    padding: 8px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .search-section {
    padding: 8px;
    flex-shrink: 0;
  }
</style>

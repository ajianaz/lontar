<script lang="ts">
  import { onMount } from 'svelte';
  import type { Backlink } from '../ts/types';
  import { getVaultStore } from '../stores/vault.svelte';
  import { getEditorStore } from '../stores/editor.svelte';
  import { indexer } from '../ts/ipc';

  const vault = getVaultStore();
  const editor = getEditorStore();

  let backlinks = $state<Backlink[]>([]);
  let loading = $state(false);

  $effect(() => {
    const path = vault.currentNotePath;
    if (path) {
      loadBacklinks(path);
    } else {
      backlinks = [];
    }
  });

  async function loadBacklinks(path: string) {
    loading = true;
    try {
      backlinks = await indexer.getBacklinks(path);
    } catch {
      backlinks = [];
    } finally {
      loading = false;
    }
  }

  function openBacklink(path: string) {
    editor.openTab(path);
  }
</script>

<div class="backlinks-panel">
  {#if loading}
    <p class="empty">Loading...</p>
  {:else if backlinks.length === 0}
    <p class="empty">No backlinks</p>
  {:else}
    <ul class="backlink-list">
      {#each backlinks as bl}
        <li class="backlink-item" onclick={() => openBacklink(bl.source_path)}>
          <span class="backlink-title">{bl.source_title}</span>
          <span class="backlink-context">{bl.context}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .backlinks-panel { padding: 4px 0; }
  .empty {
    color: var(--text-muted);
    font-size: 0.8rem;
    padding: 12px;
    text-align: center;
  }
  .backlink-list { list-style: none; margin: 0; padding: 0; }
  .backlink-item {
    padding: 8px 12px;
    cursor: pointer;
    border-radius: 4px;
    transition: background 0.1s;
    border-bottom: 1px solid var(--border);
  }
  .backlink-item:last-child { border-bottom: none; }
  .backlink-item:hover { background: var(--bg-hover); }
  .backlink-title {
    display: block;
    font-size: 0.8rem;
    color: var(--text-primary);
    font-weight: 500;
    margin-bottom: 2px;
  }
  .backlink-context {
    display: block;
    font-size: 0.7rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

<script lang="ts">
  import { getVaultStore } from '../stores/vault.svelte';

  const vault = getVaultStore();

  let tags = $derived(vault.currentNoteData?.tags ?? []);
</script>

<div class="tags-panel">
  {#if tags.length === 0}
    <p class="empty">No tags</p>
  {:else}
    <div class="tag-list">
      {#each tags as tag}
        <span class="tag">#{tag}</span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .tags-panel { padding: 12px; }
  .empty {
    color: var(--text-muted);
    font-size: 0.8rem;
    text-align: center;
  }
  .tag-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .tag {
    display: inline-block;
    padding: 2px 8px;
    background: var(--bg-active);
    color: var(--accent);
    border-radius: 4px;
    font-size: 0.75rem;
    cursor: default;
    transition: background 0.1s;
  }
  .tag:hover { background: rgba(137, 180, 250, 0.15); }
</style>

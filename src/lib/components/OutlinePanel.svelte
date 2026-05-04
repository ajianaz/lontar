<script lang="ts">
  import type { Heading } from '../ts/types';
  import { getVaultStore } from '../stores/vault.svelte';

  const vault = getVaultStore();

  let headings = $derived(vault.currentNoteData?.headings ?? []);
</script>

<div class="outline-panel">
  {#if headings.length === 0}
    <p class="empty">No headings</p>
  {:else}
    <ul class="heading-list">
      {#each headings as heading}
        <li
          class="heading-item"
          style="padding-left: {(heading.level - 1) * 12 + 8}px"
        >
          <span class="heading-text">{heading.text}</span>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .outline-panel { padding: 4px 0; }
  .empty {
    color: var(--text-muted);
    font-size: 0.8rem;
    padding: 12px;
    text-align: center;
  }
  .heading-list { list-style: none; margin: 0; padding: 0; }
  .heading-item {
    padding: 4px 8px;
    font-size: 0.8rem;
    color: var(--text-secondary);
    cursor: default;
    border-radius: 4px;
    transition: background 0.1s;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .heading-item:hover { background: var(--bg-hover); color: var(--text-primary); }
  .heading-text { overflow: hidden; text-overflow: ellipsis; }
</style>

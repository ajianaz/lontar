<script lang="ts">
  import { getEditorStore } from '../stores/editor.svelte';
  const editor = getEditorStore();
</script>

{#if editor.tabs.length > 0}
  <div class="tab-bar">
    {#each editor.tabs as tab, i}
      <button
        class="tab"
        class:active={i === editor.activeTabIndex}
        class:dirty={tab.isDirty}
        onclick={() => editor.setActiveTabIndex(i)}
      >
        <span class="tab-title">{tab.isDirty ? '● ' : ''}{tab.title}</span>
        <span class="tab-close" onclick={(e) => { e.stopPropagation(); editor.closeTab(i); }} title="Close">✕</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .tab-bar {
    display: flex;
    height: 36px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    flex-shrink: 0;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
    background: transparent;
    border: none;
    border-right: 1px solid var(--border);
    color: var(--text-muted);
    font-size: 0.8rem;
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
    white-space: nowrap;
    min-width: 80px;
    max-width: 180px;
  }
  .tab:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .tab.active { background: var(--bg-primary); color: var(--text-primary); }
  .tab-title { overflow: hidden; text-overflow: ellipsis; }
  .tab-close {
    font-size: 0.7rem;
    padding: 1px 4px;
    border-radius: 3px;
    opacity: 0;
    transition: opacity 0.1s, background 0.1s;
  }
  .tab:hover .tab-close { opacity: 1; }
  .tab-close:hover { background: var(--bg-active); }
</style>

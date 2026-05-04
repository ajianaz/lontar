<script lang="ts">
  import { getUiStore } from '../stores/ui.svelte';
  import OutlinePanel from './OutlinePanel.svelte';
  import BacklinksPanel from './BacklinksPanel.svelte';
  import TagsPanel from './TagsPanel.svelte';

  const ui = getUiStore();

  const tabs = [
    { id: 'outline' as const, label: 'Outline' },
    { id: 'backlinks' as const, label: 'Backlinks' },
    { id: 'tags' as const, label: 'Tags' },
  ];
</script>

{#if ui.sidePanelVisible}
  <aside class="side-panel" style="width: {ui.sidePanelWidth}px">
    <div class="panel-tabs">
      {#each tabs as tab}
        <button
          class="panel-tab"
          class:active={ui.sidePanelTab === tab.id}
          onclick={() => ui.setSidePanelTab(tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
    <div class="panel-content">
      {#if ui.sidePanelTab === 'outline'}
        <OutlinePanel />
      {:else if ui.sidePanelTab === 'backlinks'}
        <BacklinksPanel />
      {:else}
        <TagsPanel />
      {/if}
    </div>
  </aside>
{/if}

<style>
  .side-panel {
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    background: var(--bg-secondary);
    flex-shrink: 0;
    overflow: hidden;
  }
  .panel-tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }
  .panel-tab {
    flex: 1;
    padding: 8px 12px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    transition: color 0.1s, background 0.1s;
    border-bottom: 2px solid transparent;
  }
  .panel-tab:hover { color: var(--text-secondary); background: var(--bg-hover); }
  .panel-tab.active { color: var(--accent); border-bottom-color: var(--accent); }
  .panel-content {
    flex: 1;
    overflow-y: auto;
  }
</style>

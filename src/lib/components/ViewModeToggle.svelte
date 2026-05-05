<script lang="ts">
  import { getUiStore } from '../stores/ui.svelte';
  import type { ViewMode } from '../stores/ui.svelte';

  const ui = getUiStore();

  type ModeDef = { mode: ViewMode; label: string; icon: string };
  const modes: ModeDef[] = [
    { mode: 'edit', label: 'Edit', icon: '✏️' },
    { mode: 'split', label: 'Split', icon: '◫' },
    { mode: 'preview', label: 'Preview', icon: '👁' },
  ];
</script>

<div class="view-mode-toggle">
  {#each modes as m}
    <button
      class="toggle-btn"
      class:active={ui.viewMode === m.mode}
      onclick={() => ui.setViewMode(m.mode)}
      title={m.label}
    >
      <span class="btn-icon">{m.icon}</span>
      <span class="btn-label">{m.label}</span>
    </button>
  {/each}
</div>

<style>
  .view-mode-toggle {
    display: flex;
    gap: 2px;
    background: var(--bg-secondary);
    border-radius: 6px;
    padding: 2px;
    border: 1px solid var(--border);
  }
  .toggle-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 0.7rem;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .toggle-btn:hover {
    color: var(--text-secondary);
    background: var(--bg-tertiary);
  }
  .toggle-btn.active {
    color: var(--text-primary);
    background: var(--bg-tertiary);
  }
  .btn-icon {
    font-size: 0.75rem;
    line-height: 1;
  }
  .btn-label {
    line-height: 1;
  }
</style>

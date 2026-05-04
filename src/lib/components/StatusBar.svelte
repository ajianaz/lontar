<script lang="ts">
  import { getVaultStore } from '../stores/vault.svelte';
  import { getEditorStore } from '../stores/editor.svelte';
  import { APP_NAME } from '../ts/constants';

  const vault = getVaultStore();
  const editor = getEditorStore();
</script>

<footer class="status-bar">
  <span class="left">{APP_NAME}</span>
  <span class="center">
    {#if vault.error}
      <span class="error">⚠ {vault.error}</span>
    {:else if vault.currentNoteData}
      {vault.currentNoteData.word_count} words · {vault.currentNoteData.line_count} lines
    {/if}
  </span>
  <span class="right">
    {#if editor.activeTab?.isDirty}
      <span class="dirty">Modified</span>
    {/if}
  </span>
</footer>

<style>
  .status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 24px;
    padding: 0 12px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border);
    font-size: 0.7rem;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .left { font-weight: 600; color: var(--accent); }
  .center { flex: 1; text-align: center; }
  .error { color: #f38ba8; }
  .dirty { color: #fab387; }
  .right { min-width: 80px; text-align: right; }
</style>

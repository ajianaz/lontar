<script lang="ts">
  import { onMount } from 'svelte';
  import { getVaultStore } from '../stores/vault.svelte';
  import { getEditorStore } from '../stores/editor.svelte';
  import { createEditorSetup } from '../cm6/editor';

  const vault = getVaultStore();
  const editor = getEditorStore();

  let container: HTMLDivElement;
  let editorApi: { view: any; destroy: () => void; setContent: (doc: string) => void } | null = null;

  onMount(() => {
    if (container) {
      editorApi = createEditorSetup(container, {
        onChange: (content) => {
          vault.setCurrentNoteContent(content);
          editor.markDirty();
          editor.scheduleSave(content);
        },
        onSave: () => {
          if (editor.activeTab) {
            editor.flushSave();
            // Immediate save
            if (vault.currentNotePath && vault.currentNoteContent) {
              import('../ts/ipc').then(({ vault: vaultApi }) => {
                vaultApi.updateNote(vault.currentNotePath!, vault.currentNoteContent)
                  .then(() => editor.markClean()); // clear dirty after save
              });
            }
          }
        },
      });
    }
    return () => editorApi?.destroy();
  });

  // Load content when active tab changes
  $effect(() => {
    const path = vault.currentNotePath;
    const content = vault.currentNoteContent;
    if (editorApi && path && content !== undefined) {
      // Only set content if editor is empty or different note
      editorApi.setContent(content);
    }
  });
</script>

<div class="editor-pane" bind:this={container}></div>

<style>
  .editor-pane {
    flex: 1;
    overflow: hidden;
    height: 100%;
  }
  .editor-pane :global(.cm-editor) {
    height: 100%;
  }
</style>

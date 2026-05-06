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
            // Capture identity of the tab being saved BEFORE async call.
            // If the user switches tabs before updateNote resolves,
            // we must only mark the saved tab as clean — not the new active.
            const savedPath = vault.currentNotePath;
            const savedContent = vault.currentNoteContent;
            if (savedPath && savedContent !== undefined) {
              import('../ts/ipc').then(({ vault: vaultApi }) => {
                vaultApi.updateNote(savedPath, savedContent)
                  .then(() => {
                    // Only clear dirty if this tab is still active
                    if (editor.activeTab?.path === savedPath) {
                      editor.markClean();
                    }
                  })
                  .catch((err) => {
                    // keep dirty state on failure — user can retry
                    console.error('[EditorPane] save failed:', err);
                  });
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

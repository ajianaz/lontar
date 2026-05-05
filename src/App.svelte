<script lang="ts">
  import { onMount } from 'svelte';
  import './app.css';
  import { getVaultStore } from './lib/stores/vault.svelte';
  import { getEditorStore } from './lib/stores/editor.svelte';
  import { getUiStore } from './lib/stores/ui.svelte';
  import { getSearchStore } from './lib/stores/search.svelte';
  import { watcher, listen } from './lib/ts/ipc';
  import type { WatchEvent, UnlistenFn } from './lib/ts/ipc';
  import Sidebar from './lib/components/Sidebar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import EditorPane from './lib/components/EditorPane.svelte';
  import SidePanel from './lib/components/SidePanel.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import PreviewPane from './lib/components/PreviewPane.svelte';
  import ViewModeToggle from './lib/components/ViewModeToggle.svelte';
  import GraphView from './lib/components/GraphView.svelte';

  const vault = getVaultStore();
  const editor = getEditorStore();
  const ui = getUiStore();
  const search = getSearchStore();

  let unlistenWatch: UnlistenFn | null = null;

  onMount(async () => {
    // Listen for file system watch events from Rust backend
    unlistenWatch = await listen<WatchEvent>('vault-change', (event) => {
      vault.handleWatchEvent(event.payload);
    });

    // Global keyboard shortcuts
    const handleGlobalKeydown = (e: KeyboardEvent) => {
      // Ctrl+B: toggle sidebar
      if (e.key === 'b' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        ui.toggleSidebar();
      }
      // Ctrl+\: toggle side panel
      if (e.key === '\\' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        ui.toggleSidePanel();
      }
      // Ctrl+N: new note (when vault is open)
      if (e.key === 'n' && (e.ctrlKey || e.metaKey) && !e.shiftKey) {
        e.preventDefault();
        // TODO: trigger new note flow via command palette
      }
      // Ctrl+P: command palette
      if (e.key === 'p' && (e.ctrlKey || e.metaKey) && !e.shiftKey) {
        e.preventDefault();
        ui.setCommandPaletteOpen(!ui.commandPaletteOpen);
      }
      // Ctrl+Shift+E: cycle view mode
      if (e.key === 'E' && (e.ctrlKey || e.metaKey) && e.shiftKey) {
        e.preventDefault();
        ui.cycleViewMode();
      }
      // Ctrl+G: toggle graph view
      if (e.key === 'g' && (e.ctrlKey || e.metaKey) && !e.shiftKey) {
        e.preventDefault();
        ui.toggleGraphView();
      }
    };

    document.addEventListener('keydown', handleGlobalKeydown);

    return () => {
      unlistenWatch?.();
      document.removeEventListener('keydown', handleGlobalKeydown);
    };
  });

  // Start watcher when vault opens, stop on close
  $effect(() => {
    if (vault.isOpen) {
      watcher.start().catch(() => {});
    } else {
      watcher.stop().catch(() => {});
    }
  });

  // Click on empty area: clear search
  function handleMainClick() {
    search.clearResults();
  }
</script>

<div class="app">
  {#if vault.isOpen}
    <div class="main-layout">
      <Sidebar />
      <div class="editor-area">
        {#if ui.graphViewVisible}
          <GraphView />
        {/if}
        <TabBar />
        {#if editor.activeTab}
          <div class="editor-toolbar">
            <ViewModeToggle />
          </div>
          <div class="editor-container">
            {#if ui.viewMode === 'edit'}
              <div class="editor-pane-wrap"><EditorPane /></div>
              <SidePanel />
            {:else if ui.viewMode === 'preview'}
              <div class="preview-pane-wrap"><PreviewPane /></div>
              <SidePanel />
            {:else}
              <div class="split-editor">
                <div class="split-left"><EditorPane /></div>
                <div class="split-divider"></div>
                <div class="split-right"><PreviewPane /></div>
              </div>
              <SidePanel />
            {/if}
          </div>
        {:else}
          <div class="empty-state">
            <div class="empty-icon">📝</div>
            <p>Select a note to start editing</p>
            <p class="empty-hint">Ctrl+K to search · Ctrl+B to toggle sidebar</p>
          </div>
        {/if}
      </div>
    </div>
    <StatusBar />
  {:else}
    <div class="welcome-screen">
      <div class="welcome-content">
        <h1 class="app-title">Lontar</h1>
        <p class="app-subtitle">A local-first knowledge base</p>
      </div>
    </div>
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    background: var(--bg-primary);
    color: var(--text-primary);
  }
  .main-layout {
    display: flex;
    flex: 1;
    overflow: hidden;
  }
  .editor-area {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }
  .editor-toolbar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-secondary);
    flex-shrink: 0;
  }
  .editor-container {
    flex: 1;
    display: flex;
    overflow: hidden;
  }
  .editor-pane-wrap,
  .preview-pane-wrap {
    flex: 1;
    overflow: hidden;
    min-width: 0;
  }
  .split-editor {
    flex: 1;
    display: flex;
    overflow: hidden;
    min-width: 0;
  }
  .split-left,
  .split-right {
    flex: 1;
    overflow: hidden;
    min-width: 0;
  }
  .split-divider {
    width: 1px;
    background: var(--border);
    flex-shrink: 0;
  }
  .welcome-screen {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .welcome-content {
    text-align: center;
  }
  .app-title {
    font-size: 2rem;
    font-weight: 700;
    color: var(--accent);
    margin: 0 0 8px 0;
    letter-spacing: 2px;
  }
  .app-subtitle {
    color: var(--text-muted);
    font-size: 0.9rem;
    margin: 0;
  }
  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    color: var(--text-muted);
  }
  .empty-icon { font-size: 2.5rem; opacity: 0.5; }
  .empty-state p { margin: 0; font-size: 0.9rem; }
  .empty-hint { font-size: 0.75rem !important; opacity: 0.6; }
</style>

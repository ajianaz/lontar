<script lang="ts">
  // Svelte 5 runes
  let sidebarWidth = $state(260);
  let sidePanelWidth = $state(300);
  let activeNote = $state<string | null>(null);

  let notes = $state<string[]>([
    "Welcome.md",
    "Getting Started.md",
    "Changelog.md",
  ]);

  let selectedNote = $state("Welcome.md");

  let notesList = $derived(notes);

  $effect(() => {
    if (selectedNote) {
      activeNote = selectedNote;
    }
  });

  function selectNote(note: string) {
    selectedNote = note;
  }
</script>

<div class="app">
  <!-- Sidebar -->
  <aside
    class="sidebar"
    style="width: {sidebarWidth}px;"
  >
    <header class="panel-header">
      <h1 class="app-title">Lontar</h1>
    </header>
    <div class="search-box">
      <input type="text" placeholder="Search notes…" />
    </div>
    <nav class="note-list">
      {#each notesList as note}
        <button
          class="note-item"
          class:active={selectedNote === note}
          onclick={() => selectNote(note)}
        >
          {note}
        </button>
      {/each}
    </nav>
    <footer class="panel-footer">
      <button class="new-note-btn">+ New Note</button>
    </footer>
  </aside>

  <!-- Resize: sidebar | editor -->
  <div class="resize-handle" data-resize="sidebar"></div>

  <!-- Editor -->
  <main class="editor">
    {#if activeNote}
      <div class="editor-toolbar">
        <span class="editor-filename">{activeNote}</span>
      </div>
      <div class="editor-content">
        <textarea placeholder="Start writing…"></textarea>
      </div>
    {:else}
      <div class="editor-empty">
        <p>Select a note or create a new one</p>
      </div>
    {/if}
  </main>

  <!-- Resize: editor | side panel -->
  <div class="resize-handle" data-resize="side-panel"></div>

  <!-- Side Panel -->
  <aside
    class="side-panel"
    style="width: {sidePanelWidth}px;"
  >
    <header class="panel-header">
      <h2>Outline</h2>
    </header>
    <div class="side-panel-content">
      <p class="placeholder-text">Note outline will appear here.</p>
    </div>
  </aside>
</div>

<style>
  :root {
    --bg-primary: #1e1e2e;
    --bg-secondary: #181825;
    --bg-tertiary: #11111b;
    --bg-hover: #313244;
    --bg-active: #45475a;
    --text-primary: #cdd6f4;
    --text-secondary: #a6adc8;
    --text-muted: #6c7086;
    --accent: #89b4fa;
    --accent-hover: #74c7ec;
    --border: #313244;
    --border-light: #45475a;
    --resize-handle: #585b70;
    --scrollbar-thumb: #45475a;
    --scrollbar-track: transparent;
    --font-mono: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
    --font-sans: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
    --sidebar-width: 260px;
    --side-panel-width: 300px;
    --panel-header-height: 48px;
  }

  * {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
  }

  :global(body) {
    font-family: var(--font-sans);
    background: var(--bg-primary);
    color: var(--text-primary);
    overflow: hidden;
    height: 100vh;
    width: 100vw;
  }

  :global(::-webkit-scrollbar) {
    width: 6px;
  }
  :global(::-webkit-scrollbar-track) {
    background: var(--scrollbar-track);
  }
  :global(::-webkit-scrollbar-thumb) {
    background: var(--scrollbar-thumb);
    border-radius: 3px;
  }

  .app {
    display: grid;
    grid-template-columns: auto 4px 1fr 4px auto;
    height: 100vh;
    width: 100vw;
  }

  /* ── Sidebar ── */
  .sidebar {
    background: var(--bg-secondary);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 180px;
  }

  .panel-header {
    height: var(--panel-header-height);
    display: flex;
    align-items: center;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .app-title {
    font-size: 1.1rem;
    font-weight: 700;
    color: var(--accent);
    letter-spacing: 0.5px;
  }

  .panel-header h2 {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .search-box {
    padding: 8px 12px;
    flex-shrink: 0;
  }

  .search-box input {
    width: 100%;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 0.85rem;
    outline: none;
    transition: border-color 0.15s;
  }

  .search-box input:focus {
    border-color: var(--accent);
  }

  .search-box input::placeholder {
    color: var(--text-muted);
  }

  .note-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 8px;
  }

  .note-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 12px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.85rem;
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
    margin-bottom: 2px;
  }

  .note-item:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .note-item.active {
    background: var(--bg-active);
    color: var(--text-primary);
    font-weight: 500;
  }

  .panel-footer {
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }

  .new-note-btn {
    width: 100%;
    padding: 8px;
    border: 1px dashed var(--border-light);
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font-size: 0.8rem;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
  }

  .new-note-btn:hover {
    color: var(--accent);
    border-color: var(--accent);
  }

  /* ── Resize Handle ── */
  .resize-handle {
    background: var(--resize-handle);
    cursor: col-resize;
    transition: background 0.15s;
    position: relative;
    z-index: 10;
  }

  .resize-handle:hover,
  .resize-handle:active {
    background: var(--accent);
  }

  /* ── Editor ── */
  .editor {
    background: var(--bg-primary);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 300px;
  }

  .editor-toolbar {
    height: var(--panel-header-height);
    display: flex;
    align-items: center;
    padding: 0 20px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .editor-filename {
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .editor-content {
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  .editor-content textarea {
    flex: 1;
    padding: 20px;
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 0.9rem;
    line-height: 1.7;
    resize: none;
    outline: none;
    tab-size: 2;
  }

  .editor-content textarea::placeholder {
    color: var(--text-muted);
  }

  .editor-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .editor-empty p {
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  /* ── Side Panel ── */
  .side-panel {
    background: var(--bg-secondary);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 180px;
  }

  .side-panel-content {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
  }

  .placeholder-text {
    color: var(--text-muted);
    font-size: 0.8rem;
    font-style: italic;
  }
</style>

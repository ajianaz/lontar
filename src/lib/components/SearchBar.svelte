<script lang="ts">
  import { onMount } from 'svelte';
  import { getSearchStore } from '../stores/search.svelte';
  import { getVaultStore } from '../stores/vault.svelte';
  import { getEditorStore } from '../stores/editor.svelte';

  const search = getSearchStore();
  const vault = getVaultStore();
  const editor = getEditorStore();

  let inputEl: HTMLInputElement;
  let isFocused = $state(false);
  let selectedIdx = $state(-1);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'k' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      inputEl?.focus();
      return;
    }
    if (e.key === 'Escape') {
      inputEl?.blur();
      search.clearResults();
      return;
    }
    if (isFocused && search.results.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        selectedIdx = Math.min(selectedIdx + 1, search.results.length - 1);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        selectedIdx = Math.max(selectedIdx - 1, 0);
      } else if (e.key === 'Enter' && selectedIdx >= 0) {
        e.preventDefault();
        selectResult(search.results[selectedIdx]);
      }
    }
  }

  function handleInput(e: Event) {
    const value = (e.target as HTMLInputElement).value;
    search.performSearch(value);
    selectedIdx = -1;
  }

  function selectResult(result: SearchResult) {
    editor.openTab(result.path);
    search.clearResults();
    inputEl?.blur();
    selectedIdx = -1;
  }

  onMount(() => {
    document.addEventListener('keydown', handleKeydown);
    return () => document.removeEventListener('keydown', handleKeydown);
  });
</script>

<div class="search-bar">
  <div class="search-input-wrapper">
    <span class="search-icon">🔍</span>
    <input
      bind:this={inputEl}
      type="text"
      class="search-input"
      placeholder="Search notes... (Ctrl+K)"
      value={search.query}
      oninput={handleInput}
      onfocus={() => isFocused = true}
      onblur={() => setTimeout(() => { isFocused = false; selectedIdx = -1; }, 200)}
    />
  </div>
  {#if isFocused && search.results.length > 0}
    <ul class="search-results">
      {#each search.results as result, i}
        <li
          class="result-item"
          class:selected={i === selectedIdx}
          onclick={() => selectResult(result)}
        >
          <div class="result-title">{result.title}</div>
          <div class="result-snippet">{result.snippet}</div>
          {#if result.tags.length > 0}
            <div class="result-tags">
              {#each result.tags as tag}
                <span class="result-tag">#{tag}</span>
              {/each}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if isFocused && search.query && search.isSearching}
    <div class="search-loading">Searching...</div>
  {/if}
</div>

<style>
  .search-bar { position: relative; }
  .search-input-wrapper {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 6px;
    transition: border-color 0.15s;
  }
  .search-input-wrapper:focus-within { border-color: var(--accent); }
  .search-icon { font-size: 0.8rem; opacity: 0.5; }
  .search-input {
    flex: 1;
    padding: 6px 4px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: 0.8rem;
    outline: none;
  }
  .search-input::placeholder { color: var(--text-muted); }
  .search-results {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    margin-top: 4px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 8px;
    max-height: 300px;
    overflow-y: auto;
    z-index: 100;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
    padding: 4px;
  }
  .result-item {
    padding: 8px 10px;
    cursor: pointer;
    border-radius: 4px;
    transition: background 0.1s;
  }
  .result-item:hover, .result-item.selected { background: var(--bg-hover); }
  .result-title {
    font-size: 0.8rem;
    font-weight: 500;
    color: var(--text-primary);
    margin-bottom: 2px;
  }
  .result-snippet {
    font-size: 0.7rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .result-tags { display: flex; gap: 4px; margin-top: 4px; }
  .result-tag {
    font-size: 0.65rem;
    color: var(--accent);
    opacity: 0.8;
  }
  .search-loading {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    text-align: center;
    padding: 8px;
    color: var(--text-muted);
    font-size: 0.75rem;
  }
</style>

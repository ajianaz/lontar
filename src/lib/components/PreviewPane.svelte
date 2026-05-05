<script lang="ts">
  import { marked } from 'marked';
  import { getVaultStore } from '../stores/vault.svelte';

  const vault = getVaultStore();

  // Configure marked once (module-level, safe in Svelte 5)
  let _configured = false;
  if (!_configured) {
    marked.setOptions({ gfm: true, breaks: true });
    const wikilinkExt = {
      name: 'wikilink',
      level: 'inline' as const,
      start(src: string) { return src.indexOf('[['); },
      tokenizer(src: string) {
        const match = src.match(/^\[\[([^\]]+)\]\]/);
        if (match) {
          return { type: 'wikilink', raw: match[0], text: match[1] };
        }
        return undefined;
      },
      renderer(token: { text: string }) {
        return `<span class="wikilink">${token.text}</span>`;
      },
    };
    marked.use({ extensions: [wikilinkExt as any] });
    _configured = true;
  }

  // marked() is sync when no async extensions are used
  let rendered = $derived(vault.currentNoteContent
    ? marked.parse(vault.currentNoteContent) as string
    : ''
  );
</script>

<div class="preview-pane">
  {#if vault.currentNoteContent}
    <div class="preview-content">
      {@html rendered}
    </div>
  {:else}
    <div class="preview-empty">No note selected</div>
  {/if}
</div>

<style>
  .preview-pane {
    flex: 1;
    overflow-y: auto;
    padding: 0;
    height: 100%;
  }

  .preview-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-muted);
    font-size: 0.9rem;
  }

  .preview-content {
    max-width: 720px;
    margin: 0 auto;
    padding: 24px 32px;
    color: var(--text-primary);
    line-height: 1.7;
    font-size: 0.95rem;
  }

  /* Headings */
  .preview-content :global(h1),
  .preview-content :global(h2),
  .preview-content :global(h3),
  .preview-content :global(h4),
  .preview-content :global(h5),
  .preview-content :global(h6) {
    color: var(--text-primary);
    margin-top: 1.5em;
    margin-bottom: 0.5em;
    font-weight: 600;
    line-height: 1.3;
  }
  .preview-content :global(h1) { font-size: 1.8rem; border-bottom: 1px solid var(--border); padding-bottom: 0.3em; }
  .preview-content :global(h2) { font-size: 1.4rem; border-bottom: 1px solid var(--border); padding-bottom: 0.25em; }
  .preview-content :global(h3) { font-size: 1.2rem; }
  .preview-content :global(h4) { font-size: 1.05rem; }
  .preview-content :global(h1:first-child),
  .preview-content :global(h2:first-child),
  .preview-content :global(h3:first-child) { margin-top: 0; }

  /* Paragraphs */
  .preview-content :global(p) { margin: 0 0 0.75em 0; }

  /* Links */
  .preview-content :global(a) {
    color: var(--accent);
    text-decoration: none;
    border-bottom: 1px solid transparent;
    transition: border-color 0.15s;
  }
  .preview-content :global(a:hover) {
    border-bottom-color: var(--accent);
  }

  /* Wikilinks */
  .preview-content :global(.wikilink) {
    color: var(--accent);
    text-decoration: underline dotted;
    cursor: pointer;
    background: rgba(137, 180, 250, 0.08);
    padding: 1px 3px;
    border-radius: 3px;
  }

  /* Bold & italic */
  .preview-content :global(strong) { color: var(--text-primary); font-weight: 600; }
  .preview-content :global(em) { color: var(--text-secondary); }

  /* Code */
  .preview-content :global(code) {
    font-family: var(--font-mono);
    font-size: 0.85em;
    background: var(--bg-tertiary);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--peach);
  }
  .preview-content :global(pre) {
    background: var(--bg-tertiary);
    border-radius: 8px;
    padding: 16px;
    overflow-x: auto;
    margin: 0 0 1em 0;
    border: 1px solid var(--border);
  }
  .preview-content :global(pre code) {
    background: none;
    padding: 0;
    border-radius: 0;
    color: var(--text-primary);
    font-size: 0.85rem;
    line-height: 1.6;
  }

  /* Blockquotes */
  .preview-content :global(blockquote) {
    border-left: 3px solid var(--accent);
    margin: 0 0 1em 0;
    padding: 8px 16px;
    background: rgba(137, 180, 250, 0.05);
    color: var(--text-secondary);
    border-radius: 0 6px 6px 0;
  }
  .preview-content :global(blockquote p:last-child) { margin-bottom: 0; }

  /* Lists */
  .preview-content :global(ul),
  .preview-content :global(ol) {
    padding-left: 1.5em;
    margin: 0 0 0.75em 0;
  }
  .preview-content :global(li) { margin-bottom: 0.25em; }
  .preview-content :global(li > ul),
  .preview-content :global(li > ol) { margin-bottom: 0; margin-top: 0.25em; }

  /* Task lists (GFM) */
  .preview-content :global(input[type="checkbox"]) {
    margin-right: 6px;
    accent-color: var(--accent);
  }

  /* Tables */
  .preview-content :global(table) {
    width: 100%;
    border-collapse: collapse;
    margin: 0 0 1em 0;
    font-size: 0.9rem;
  }
  .preview-content :global(th),
  .preview-content :global(td) {
    border: 1px solid var(--border);
    padding: 8px 12px;
    text-align: left;
  }
  .preview-content :global(th) {
    background: var(--bg-tertiary);
    font-weight: 600;
    color: var(--text-primary);
  }
  .preview-content :global(tr:nth-child(even)) {
    background: rgba(49, 50, 68, 0.4);
  }

  /* Images */
  .preview-content :global(img) {
    max-width: 100%;
    border-radius: 8px;
    margin: 0.5em 0;
  }

  /* Horizontal rule */
  .preview-content :global(hr) {
    border: none;
    border-top: 1px solid var(--border);
    margin: 1.5em 0;
  }

  /* Strikethrough */
  .preview-content :global(del) {
    color: var(--text-muted);
  }
</style>

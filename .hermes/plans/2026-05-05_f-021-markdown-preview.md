# F-021: Markdown Live Preview

## Goal
Add markdown live preview to the editor — user can toggle between edit-only, preview-only, and split (edit+preview) modes.

## Approach
1. Install `marked` (lightweight markdown parser, ~40KB gzipped)
2. Create `PreviewPane.svelte` — renders markdown to HTML
3. Add view mode state to UI store (`edit`, `preview`, `split`)
4. Update `EditorPane.svelte` to respect view mode
5. Add toggle button to editor area header

## Files to Create/Modify
- **NEW:** `src/lib/components/PreviewPane.svelte`
- **MODIFY:** `src/lib/stores/ui.svelte.ts` — add `viewMode` state
- **MODIFY:** `src/lib/components/EditorPane.svelte` — respect view mode
- **MODIFY:** `src/App.svelte` — add preview pane and toggle
- **MODIFY:** `package.json` — add `marked` dependency
- **MODIFY:** `src/lib/ts/constants.ts` — add view mode type

## Design Decisions
- **Parser:** `marked` — fast, small, extensible, supports GFM
- **Highlighting:** Code blocks get basic syntax highlighting via `marked-highlight` + `highlight.js` (optional, later)
- **Wikilinks:** Custom renderer to convert `[[link]]` to clickable spans
- **Sanitization:** DOMPurify applied to rendered HTML (defense-in-depth against malformed markdown)
- **Style:** Reuse CSS variables from `app.css`, add markdown-specific styles

## View Modes
- `edit` (default) — CM6 editor only
- `preview` — rendered markdown only
- `split` — side-by-side (50/50)

## Keyboard Shortcuts
- `Ctrl+E` — toggle edit/preview
- `Ctrl+Shift+E` — cycle edit → split → preview

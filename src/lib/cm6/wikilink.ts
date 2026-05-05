import { Decoration, EditorView, ViewPlugin, ViewUpdate } from '@codemirror/view';
import type { DecorationSet } from '@codemirror/view';
import { RangeSetBuilder } from '@codemirror/state';

// Decoration for the wikilink text (target)
const linkDecoration = Decoration.mark({
  class: 'cm-wikilink',
  inclusive: false,
});

// Decoration for brackets [[ and ]]
const bracketDecoration = Decoration.mark({
  class: 'cm-wikilink-bracket',
});

// MatchDecorator not used — removed, using ViewPlugin approach only
export const wikilinkExtension = ViewPlugin.fromClass(class {
  decorations: DecorationSet;

  constructor(view: EditorView) {
    this.decorations = buildDecorations(view);
  }

  update(update: ViewUpdate) {
    if (update.docChanged || update.viewportChanged) {
      this.decorations = buildDecorations(update.view);
    }
  }
}, {
  decorations: v => v.decorations,
});

// Exported for testing — pure function, no EditorView dependency
export function findWikilinks(
  text: string,
): Array<{ start: number; end: number; content: string }> {
  const regex = /\[\[([^\]]+?)\]\]/g;
  const results: Array<{ start: number; end: number; content: string }> = [];
  let match: RegExpExecArray | null;
  while ((match = regex.exec(text)) !== null) {
    results.push({
      start: match.index,
      end: match.index + match[0].length,
      content: match[1],
    });
  }
  return results;
}

function buildDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc.toString();

  for (const { start, end } of findWikilinks(doc)) {
    // Opening bracket
    builder.add(start, start + 2, bracketDecoration);
    // Link text decoration (accent color)
    builder.add(start + 2, end - 2, linkDecoration);
    // Closing bracket
    builder.add(end - 2, end, bracketDecoration);
  }

  return builder.finish();
}

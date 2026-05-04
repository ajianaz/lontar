import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate } from '@codemirror/view';
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

function buildDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc.toString();

  const regex = /\[\[([^\]]+?)\]\]/g;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(doc)) !== null) {
    const start = match.index;
    const end = start + match[0].length;

    // Bracket decorations (subtle)
    builder.add(start, start + 2, bracketDecoration);
    builder.add(end - 2, end, bracketDecoration);

    // Link text decoration (accent color)
    builder.add(start + 2, end - 2, linkDecoration);
  }

  return builder.finish();
}

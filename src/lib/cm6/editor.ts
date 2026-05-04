import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection, rectangularSelection, highlightSpecialChars } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { languages } from '@codemirror/language-data';
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import { autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { syntaxHighlighting, defaultHighlightStyle, foldGutter, indentOnInput, bracketMatching } from '@codemirror/language';
import { wikilinkExtension } from './wikilink';

export interface EditorCallbacks {
  onChange?: (content: string) => void;
  onSave?: () => void;
}

// Create the base theme extension with custom CSS variables
const lontarTheme = EditorView.theme({
  '&': {
    height: '100%',
    fontSize: '0.9rem',
    fontFamily: 'var(--font-mono)',
    lineHeight: '1.7',
    color: 'var(--text-primary)',
    backgroundColor: 'var(--bg-primary)',
  },
  '.cm-content': {
    caretColor: 'var(--accent)',
    padding: '20px 0',
    maxWidth: '800px',
    margin: '0 auto',
  },
  '.cm-cursor, .cm-dropCursor': {
    borderLeftColor: 'var(--accent)',
    borderLeftWidth: '2px',
  },
  '.cm-activeLine': {
    backgroundColor: 'rgba(137, 180, 250, 0.06)',
  },
  '.cm-activeLineGutter': {
    backgroundColor: 'rgba(137, 180, 250, 0.1)',
    color: 'var(--text-muted)',
  },
  '.cm-selectionBackground, .cm-content ::selection': {
    backgroundColor: 'rgba(137, 180, 250, 0.2) !important',
  },
  '.cm-gutters': {
    backgroundColor: 'var(--bg-primary)',
    color: 'var(--text-muted)',
    border: 'none',
    minWidth: '40px',
  },
  '.cm-lineNumbers .cm-gutterElement': {
    fontSize: '0.75rem',
  },
  '.cm-foldGutter': {
    width: '16px',
  },
  '.cm-scroller': {
    overflow: 'auto',
    fontFamily: 'inherit',
  },
  '.cm-focused': {
    outline: 'none',
  },
  '&.cm-focused .cm-cursor': {
    borderLeftColor: 'var(--accent)',
  },
  '&.cm-focused .cm-selectionBackground, &.cm-focused .cm-content ::selection': {
    backgroundColor: 'rgba(137, 180, 250, 0.25) !important',
  },
  '.cm-panels': {
    backgroundColor: 'var(--bg-secondary)',
    borderBottom: '1px solid var(--border)',
  },
  '.cm-searchMatch': {
    backgroundColor: 'rgba(250, 179, 7, 0.3)',
    outline: '1px solid rgba(250, 179, 7, 0.5)',
  },
  '.cm-searchMatch-selected': {
    backgroundColor: 'rgba(250, 179, 7, 0.5)',
  },
  '.cm-tooltip': {
    backgroundColor: 'var(--bg-secondary)',
    border: '1px solid var(--border)',
    color: 'var(--text-primary)',
    borderRadius: '6px',
    boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
  },
  '.cm-tooltip-autocomplete ul li': {
    padding: '4px 8px',
  },
  '.cm-tooltip-autocomplete ul li[aria-selected]': {
    backgroundColor: 'var(--bg-active)',
    color: 'var(--text-primary)',
  },
  '.cm-wikilink': {
    color: 'var(--accent)',
    textDecoration: 'underline',
    textDecorationStyle: 'dotted',
    cursor: 'pointer',
  },
  '.cm-wikilink-bracket': {
    color: 'var(--text-muted)',
  },
}, { dark: true });

export function createEditorSetup(parent: HTMLElement, callbacks: EditorCallbacks = {}): { view: EditorView; destroy: () => void; setContent: (doc: string) => void } {
  const updateListener = EditorView.updateListener.of(update => {
    if (update.docChanged && callbacks.onChange) {
      callbacks.onChange(update.state.doc.toString());
    }
  });

  const saveKeymap = keymap.of([{
    key: 'Mod-s',
    run: () => {
      callbacks.onSave?.();
      return true;
    },
  }]);

  const state = EditorState.create({
    doc: '',
    extensions: [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      foldGutter(),
      drawSelection(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      bracketMatching(),
      rectangularSelection(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      closeBrackets(),
      autocompletion(),
      keymap.of(searchKeymap),
      keymap.of(historyKeymap),
      keymap.of(completionKeymap),
      keymap.of(closeBracketsKeymap),
      keymap.of([...defaultKeymap, indentWithTab]),
      saveKeymap,
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      wikilinkExtension,
      lontarTheme,
      updateListener,
      EditorView.lineWrapping,
    ],
  });

  const view = new EditorView({
    state,
    parent,
  });

  return {
    view,
    destroy: () => view.destroy(),
    setContent: (doc: string) => {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: doc },
      });
    },
  };
}

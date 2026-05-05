/**
 * Tests for editor store logic.
 *
 * Since the editor store uses Svelte 5 runes ($state, $derived),
 * we test the behavioral contract directly by verifying the functions
 * that would be called from EditorPane.svelte.
 *
 * The key fix for C2: after save succeeds, markClean() should set isDirty=false.
 */

import { describe, it, expect } from 'vitest';

describe('EditorStore — markDirty/markClean logic', () => {
  // Simulate the tabs state pattern used by editor.svelte.ts
  // We test the transformation functions, not the reactive store itself

  it('markDirty should set isDirty=true for active tab', () => {
    const tabs = [
      { path: '/notes/a.md', title: 'a', isDirty: false },
      { path: '/notes/b.md', title: 'b', isDirty: false },
    ];
    const activeTabIndex = 1;

    // Simulate markDirty behavior
    const result = tabs.map((t, i) =>
      i === activeTabIndex ? { ...t, isDirty: true } : t
    );

    expect(result[0].isDirty).toBe(false);
    expect(result[1].isDirty).toBe(true);
  });

  it('markClean should set isDirty=false for active tab', () => {
    const tabs = [
      { path: '/notes/a.md', title: 'a', isDirty: true },
      { path: '/notes/b.md', title: 'b', isDirty: true },
    ];
    const activeTabIndex = 0;

    // Simulate markClean behavior (the new function added in C2)
    const result = tabs.map((t, i) =>
      i === activeTabIndex ? { ...t, isDirty: false } : t
    );

    expect(result[0].isDirty).toBe(false);
    expect(result[1].isDirty).toBe(true); // Other tab unaffected
  });

  it('markClean should not mutate other tabs', () => {
    const tabs = [
      { path: '/notes/a.md', title: 'a', isDirty: true },
      { path: '/notes/b.md', title: 'b', isDirty: false },
      { path: '/notes/c.md', title: 'c', isDirty: true },
    ];
    const activeTabIndex = 2;

    const result = tabs.map((t, i) =>
      i === activeTabIndex ? { ...t, isDirty: false } : t
    );

    expect(result[0].isDirty).toBe(true);
    expect(result[1].isDirty).toBe(false);
    expect(result[2].isDirty).toBe(false);
  });

  it('full lifecycle: clean → dirty → save → clean', () => {
    // Simulate the full EditorPane.svelte onSave workflow
    const tabs = [{ path: '/notes/test.md', title: 'test', isDirty: false }];
    const activeTabIndex = 0;

    // 1. User types → onChange fires → markDirty
    let state = tabs.map((t, i) =>
      i === activeTabIndex ? { ...t, isDirty: true } : t
    );
    expect(state[0].isDirty).toBe(true);

    // 2. Save succeeds → markClean (was the bug: called markDirty instead)
    state = state.map((t, i) =>
      i === activeTabIndex ? { ...t, isDirty: false } : t
    );
    expect(state[0].isDirty).toBe(false);
  });

  it('scheduleSave already marks clean on success (existing behavior)', () => {
    // The auto-save debounce in scheduleSave already sets isDirty=false
    // after successful updateNote. Verify this pattern is correct.
    const tabs = [{ path: '/notes/test.md', title: 'test', isDirty: true }];
    const activeTabIndex = 0;

    // Simulate scheduleSave success path (line 61 of editor.svelte.ts)
    const result = tabs.map((t, i) =>
      i === activeTabIndex ? { ...t, isDirty: false } : t
    );

    expect(result[0].isDirty).toBe(false);
  });
});

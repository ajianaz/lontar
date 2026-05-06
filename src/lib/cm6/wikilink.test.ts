import { describe, it, expect } from 'vitest';
import { findWikilinks } from './wikilink';

describe('findWikilinks — pure logic', () => {
  it('should find a single wikilink', () => {
    const results = findWikilinks('Some text [[My Note]] more text');
    expect(results).toHaveLength(1);
    expect(results[0]).toEqual({ start: 10, end: 21, content: 'My Note' });
  });

  it('should find multiple wikilinks', () => {
    const results = findWikilinks('See [[Alpha]] and [[Beta]] for details');
    expect(results).toHaveLength(2);
    expect(results[0]).toEqual({ start: 4, end: 13, content: 'Alpha' });
    expect(results[1]).toEqual({ start: 18, end: 26, content: 'Beta' });
  });

  it('should return empty for plain text', () => {
    expect(findWikilinks('No wikilinks here')).toHaveLength(0);
  });

  it('should handle adjacent wikilinks', () => {
    const results = findWikilinks('[[A]][[B]]');
    expect(results).toHaveLength(2);
    expect(results[0]).toEqual({ start: 0, end: 5, content: 'A' });
    expect(results[1]).toEqual({ start: 5, end: 10, content: 'B' });
  });

  it('should handle wikilink with alias', () => {
    const results = findWikilinks('Check [[Target|Alias]] here');
    expect(results).toHaveLength(1);
    expect(results[0]).toEqual({ start: 6, end: 22, content: 'Target|Alias' });
  });

  it('should handle wikilinks in different positions', () => {
    const results = findWikilinks('[[start]]middle[[end]]');
    expect(results).toHaveLength(2);
    expect(results[0]).toEqual({ start: 0, end: 9, content: 'start' });
    expect(results[1]).toEqual({ start: 15, end: 22, content: 'end' });
  });

  it('should handle multiline content inside wikilinks', () => {
    const results = findWikilinks('[[line1\nline2]]');
    expect(results).toHaveLength(1);
    expect(results[0].content).toBe('line1\nline2');
  });

  it('should not match incomplete brackets', () => {
    expect(findWikilinks('[[incomplete')).toHaveLength(0);
    expect(findWikilinks('incomplete]]')).toHaveLength(0);
    expect(findWikilinks('[]empty[]')).toHaveLength(0);
  });

  it('should produce ranges in ascending order (RangeSetBuilder requirement)', () => {
    const text = '[[First]] text [[Second]] [[Third]]';
    const results = findWikilinks(text);

    for (let i = 1; i < results.length; i++) {
      expect(results[i].start).toBeGreaterThan(results[i - 1].start);
      expect(results[i].end).toBeGreaterThan(results[i - 1].end);
    }
  });

  it('should validate decoration range ordering for adjacent wikilinks', () => {
    // Adjacent wikilinks produce decorations like:
    // [[A]][[B]] → [0,2][2,4][4,6][6,8][8,10][10,12] — must be ascending
    const results = findWikilinks('[[A]][[B]]');

    const decorationRanges: { from: number; to: number }[] = [];
    for (const link of results) {
      decorationRanges.push(
        { from: link.start, to: link.start + 2 },           // [[
        { from: link.start + 2, to: link.end - 2 },         // text
        { from: link.end - 2, to: link.end },               // ]]
      );
    }

    for (let i = 1; i < decorationRanges.length; i++) {
      expect(decorationRanges[i].from, `Non-ascending range at index ${i}`)
        .toBeGreaterThanOrEqual(decorationRanges[i - 1].from);
    }
  });
});

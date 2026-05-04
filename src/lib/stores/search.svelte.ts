import { search as searchApi } from '../ts/ipc';
import type { SearchResult } from '../ts/types';
import { SEARCH_DEBOUNCE_MS } from '../ts/constants';

let query = $state('');
let results = $state<SearchResult[]>([]);
let isSearching = $state(false);
let searchTimeout: ReturnType<typeof setTimeout> | null = null;

async function performSearch(q: string) {
  query = q;
  if (!q.trim()) {
    results = [];
    return;
  }
  if (searchTimeout) clearTimeout(searchTimeout);
  searchTimeout = setTimeout(async () => {
    isSearching = true;
    try {
      results = await searchApi.search(q);
    } catch {
      results = [];
    } finally {
      isSearching = false;
    }
    searchTimeout = null;
  }, SEARCH_DEBOUNCE_MS);
}

async function instantSearch(q: string) {
  query = q;
  if (!q.trim()) { results = []; return; }
  isSearching = true;
  try {
    results = await searchApi.search(q);
  } catch {
    results = [];
  } finally {
    isSearching = false;
  }
}

function clearResults() {
  query = '';
  results = [];
}

export function getSearchStore() {
  return {
    get query() { return query; },
    get results() { return results; },
    get isSearching() { return isSearching; },
    performSearch,
    instantSearch,
    clearResults,
  };
}

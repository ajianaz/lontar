// Tree entry from get_tree command
export type TreeEntry =
  | { type: 'file'; name: string }
  | { type: 'dir'; name: string; children: TreeEntry[] };

// Note data from get_note command
export interface NoteData {
  path: string;
  title: string;
  tags: string[];
  frontmatter: Record<string, unknown> | null;
  outgoing_links: Wikilink[];
  headings: Heading[];
  created?: string;
  modified?: string;
  word_count: number;
  line_count: number;
}

export interface Wikilink {
  raw: string;
  target: string;
  heading?: string;
  display_text?: string;
  block_id?: string;
}

export interface Heading {
  level: number;
  text: string;
  slug: string;
}

export interface Backlink {
  source_path: string;
  source_title: string;
  context: string;
}

export interface GraphEdge {
  source: string;
  target: string;
}

export interface GraphData {
  nodes: { id: string; title: string }[];
  edges: GraphEdge[];
}

export interface SearchResult {
  path: string;
  title: string;
  score: number;
  snippet: string;
  tags: string[];
}

export type WatchEvent =
  | { kind: 'created' | 'modified' | 'removed'; path: string }
  | { kind: 'bulk-change'; count: number };

export interface Frontmatter {
  [key: string]: unknown;
}

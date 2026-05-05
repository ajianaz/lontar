<script lang="ts">
  import { onMount } from 'svelte';
  import * as d3 from 'd3';
  import { indexer } from '../ts/ipc';
  import { getEditorStore } from '../stores/editor.svelte';
  import { getUiStore } from '../stores/ui.svelte';
  import type { GraphData } from '../ts/types';

  const editor = getEditorStore();
  const ui = getUiStore();

  let container: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let searchInput: HTMLInputElement;

  let loading = $state(true);
  let nodes: d3.SimulationNodeDatum & { id: string; title: string; degree: number }[] = $state([]);
  let edges: d3.SimulationLinkDatum<d3.SimulationNodeDatum & { id: string; title: string; degree: number }>[] = $state([]);
  let searchTerm = $state('');
  let hoveredNode: (d3.SimulationNodeDatum & { id: string; title: string; degree: number }) | null = $state(null);
  let tooltipText = $state('');
  let tooltipX = $state(0);
  let tooltipY = $state(0);
  let width = $state(0);
  let height = $state(0);

  let simulation: d3.Simulation<
    d3.SimulationNodeDatum & { id: string; title: string; degree: number },
    d3.SimulationLinkDatum<(d3.SimulationNodeDatum & { id: string; title: string; degree: number })>
  > | null = null;

  let zoomBehavior: d3.ZoomBehavior<HTMLCanvasElement, unknown> | null = null;
  let transform = d3.zoomIdentity;
  let ctx: CanvasRenderingContext2D | null = null;

  const BASE_RADIUS = 4;
  const MAX_RADIUS = 12;
  const ACCENT = '#89b4fa';
  const ACCENT_DIM = 'rgba(137, 180, 250, 0.35)';
  const TEXT_MUTED = '#6c7086';
  const BG_SECONDARY = '#181825';
  const BORDER = '#313244';

  function degreeColor(degree: number, isHighlighted: boolean, isNeighbor: boolean, isActive: boolean, isOrphan: boolean): string {
    if (isActive) return ACCENT;
    if (isNeighbor) return 'rgba(137, 180, 250, 0.7)';
    if (isHighlighted) return ACCENT;
    if (isOrphan) return 'rgba(108, 112, 134, 0.3)';
    if (degree === 0) return TEXT_MUTED;
    return ACCENT_DIM;
  }

  function nodeRadius(degree: number): number {
    if (degree === 0) return BASE_RADIUS * 0.6;
    return BASE_RADIUS + Math.min(degree, 10) * ((MAX_RADIUS - BASE_RADIUS) / 10);
  }

  function getNeighbors(activeId: string | undefined): Set<string> {
    if (!activeId) return new Set();
    const neighbors = new Set<string>();
    neighbors.add(activeId);
    for (const edge of edges) {
      const s = (typeof edge.source === 'object' ? (edge.source as any).id : edge.source) as string;
      const t = (typeof edge.target === 'object' ? (edge.target as any).id : edge.target) as string;
      if (s === activeId) neighbors.add(t);
      if (t === activeId) neighbors.add(s);
    }
    return neighbors;
  }

  function matchesFilter(node: d3.SimulationNodeDatum & { id: string; title: string; degree: number }): boolean {
    if (!searchTerm) return true;
    return node.title.toLowerCase().includes(searchTerm.toLowerCase());
  }

  function draw() {
    if (!ctx || !canvas) return;
    const dpr = window.devicePixelRatio || 1;
    ctx!.clearRect(0, 0, canvas!.width, canvas!.height);

    const activeId = editor.activeTab?.path;
    const neighbors = getNeighbors(activeId);
    const hasActive = activeId !== undefined && activeId !== null;

    ctx!.save();
    ctx!.scale(dpr, dpr);
    ctx!.translate(transform.x, transform.y);
    ctx!.scale(transform.k, transform.k);

    // Draw edges
    for (const edge of edges) {
      const source = edge.source as d3.SimulationNodeDatum & { id: string; title: string; degree: number };
      const target = edge.target as d3.SimulationNodeDatum & { id: string; title: string; degree: number };
      const sourceVisible = matchesFilter(source);
      const targetVisible = matchesFilter(target);
      if (!sourceVisible || !targetVisible) continue;

      const isConnectedToActive = hasActive &&
        (source.id === activeId || target.id === activeId);

      ctx!.beginPath();
      ctx!.moveTo(source.x!, source.y!);
      ctx!.lineTo(target.x!, target.y!);
      if (isConnectedToActive) {
        ctx!.strokeStyle = 'rgba(137, 180, 250, 0.5)';
        ctx!.lineWidth = 1.5;
      } else if (hasActive) {
        ctx!.strokeStyle = 'rgba(108, 112, 134, 0.08)';
        ctx!.lineWidth = 0.5;
      } else {
        ctx!.strokeStyle = 'rgba(108, 112, 134, 0.2)';
        ctx!.lineWidth = 0.8;
      }
      ctx!.stroke();
    }

    // Draw nodes
    for (const node of nodes) {
      if (!matchesFilter(node)) continue;

      const isActive = hasActive && node.id === activeId;
      const isNeighbor = hasActive && neighbors.has(node.id) && !isActive;
      const isOrphan = node.degree === 0;
      const isHovered = hoveredNode && hoveredNode.id === node.id;
      const r = nodeRadius(node.degree);

      const color = degreeColor(node.degree, isHovered, isNeighbor, isActive, isOrphan);
      const dimmed = hasActive && !neighbors.has(node.id);

      ctx!.beginPath();
      ctx!.arc(node.x!, node.y!, r, 0, Math.PI * 2);
      ctx!.fillStyle = color;
      if (dimmed) {
        ctx!.globalAlpha = 0.2;
      }
      ctx!.fill();
      ctx!.globalAlpha = 1;

      // Glow for active
      if (isActive) {
        ctx!.beginPath();
        ctx!.arc(node.x!, node.y!, r + 6, 0, Math.PI * 2);
        ctx!.fillStyle = 'rgba(137, 180, 250, 0.15)';
        ctx!.fill();

        ctx!.beginPath();
        ctx!.arc(node.x!, node.y!, r + 12, 0, Math.PI * 2);
        ctx!.fillStyle = 'rgba(137, 180, 250, 0.05)';
        ctx!.fill();
      }

      // Labels (hide when zoomed out far, or for orphans when zoomed out)
      if (transform.k > 0.4 && (node.degree > 0 || transform.k > 0.8)) {
        ctx!.font = `10px var(--font-mono)`;
        ctx!.textAlign = 'center';
        ctx!.textBaseline = 'top';
        if (dimmed) {
          ctx!.fillStyle = 'rgba(108, 112, 134, 0.2)';
        } else if (isActive) {
          ctx!.fillStyle = ACCENT;
        } else if (isNeighbor) {
          ctx!.fillStyle = 'rgba(205, 214, 244, 0.7)';
        } else {
          ctx!.fillStyle = 'rgba(205, 214, 244, 0.4)';
        }
        ctx!.fillText(node.title, node.x!, node.y! + r + 4);
      }
    }

    ctx!.restore();
  }

  function loadData() {
    loading = true;
    indexer.getGraphData().then((data: GraphData) => {
      const nodeMap = new Map<string, number>();
      for (const e of data.edges) {
        nodeMap.set(e.source, (nodeMap.get(e.source) || 0) + 1);
        nodeMap.set(e.target, (nodeMap.get(e.target) || 0) + 1);
      }

      nodes = data.nodes.map(n => ({
        id: n.id,
        title: n.title || n.id.split('/').pop()?.replace('.md', '') || n.id,
        degree: nodeMap.get(n.id) || 0,
        x: Math.random() * (width || 800),
        y: Math.random() * (height || 600),
      }));

      edges = data.edges.map(e => ({
        source: e.source,
        target: e.target,
      }));

      initSimulation();
      loading = false;
    }).catch(() => {
      loading = false;
    });
  }

  function initSimulation() {
    if (simulation) simulation.stop();

    simulation = d3.forceSimulation(nodes)
      .force('link', d3.forceLink(edges).id((d: any) => d.id).distance(80))
      .force('charge', d3.forceManyBody().strength(-120))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collide', d3.forceCollide().radius((d: any) => nodeRadius(d.degree) + 8))
      .alphaDecay(0.02)
      .on('tick', draw);
  }

  function setupZoom() {
    if (!canvas) return;

    zoomBehavior = d3.zoom<HTMLCanvasElement, unknown>()
      .scaleExtent([0.1, 8])
      .on('zoom', (event: d3.D3ZoomEvent<HTMLCanvasElement, unknown>) => {
        transform = event.transform;
        draw();
      });

    d3.select(canvas).call(zoomBehavior);

    // Double-click reset
    d3.select(canvas).on('dblclick.zoom', null);
    canvas.addEventListener('dblclick', (e: MouseEvent) => {
      const rect = canvas!.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      // Check if double-click was on empty space
      const point = [x, y];
      const inverted = transform.invert(point);
      let hitNode = false;
      for (const node of nodes) {
        const dx = node.x! - inverted[0];
        const dy = node.y! - inverted[1];
        if (Math.sqrt(dx * dx + dy * dy) < nodeRadius(node.degree) + 4) {
          hitNode = true;
          break;
        }
      }
      if (!hitNode && zoomBehavior) {
        d3.select(canvas).transition().duration(500).call(zoomBehavior.transform, d3.zoomIdentity);
      }
    });
  }

  function setupInteraction() {
    if (!canvas) return;

    canvas.addEventListener('mousemove', (e: MouseEvent) => {
      const rect = canvas!.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      const inverted = transform.invert([x, y]);

      let found: typeof nodes[number] | null = null;
      for (const node of nodes) {
        if (!matchesFilter(node)) continue;
        const dx = node.x! - inverted[0];
        const dy = node.y! - inverted[1];
        if (Math.sqrt(dx * dx + dy * dy) < nodeRadius(node.degree) + 4) {
          found = node;
          break;
        }
      }

      if (found !== hoveredNode) {
        hoveredNode = found;
        if (found) {
          tooltipText = found.title;
          tooltipX = e.clientX - rect.left;
          tooltipY = e.clientY - rect.top - 12;
          canvas!.style.cursor = 'pointer';
        } else {
          tooltipText = '';
          canvas!.style.cursor = 'grab';
        }
        draw();
      }
    });

    canvas.addEventListener('click', (e: MouseEvent) => {
      if (!hoveredNode) return;
      editor.openTab(hoveredNode.id);
      ui.setGraphViewVisible(false);
    });

    canvas.addEventListener('mousedown', () => {
      if (!hoveredNode) {
        canvas!.style.cursor = 'grabbing';
      }
    });

    canvas.addEventListener('mouseup', () => {
      canvas!.style.cursor = hoveredNode ? 'pointer' : 'grab';
    });
  }

  function resizeCanvas() {
    if (!container || !canvas) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = container.getBoundingClientRect();
    width = rect.width;
    height = rect.height;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;

    if (simulation) {
      simulation.force('center', d3.forceCenter(width / 2, height / 2));
      simulation.alpha(0.3).restart();
    }
    draw();
  }

  function close() {
    ui.setGraphViewVisible(false);
  }

  function clearSearch() {
    searchTerm = '';
    if (searchInput) searchInput.focus();
    draw();
  }

  $effect(() => {
    // Redraw when active tab changes (for highlighting)
    const activePath = editor.activeTab?.path;
    void activePath; // track reactive dependency
    if (ctx) draw();
  });

  onMount(async () => {
    ctx = canvas!.getContext('2d');
    resizeCanvas();
    setupZoom();
    setupInteraction();

    const resizeObserver = new ResizeObserver(() => resizeCanvas());
    resizeObserver.observe(container);

    loadData();

    return () => {
      resizeObserver.disconnect();
      if (simulation) simulation.stop();
    };
  });
</script>

<div class="graph-container" bind:this={container}>
  {#if loading}
    <div class="loading-overlay">
      <div class="spinner"></div>
      <span>Loading graph...</span>
    </div>
  {/if}

  {#if !loading && nodes.length === 0}
    <div class="empty-overlay">
      <p>No notes yet</p>
    </div>
  {/if}

  <canvas bind:this={canvas}></canvas>

  {#if tooltipText}
    <div class="tooltip" style="left: {tooltipX}px; top: {tooltipY}px;">
      {tooltipText}
    </div>
  {/if}

  <div class="search-box">
    <input
      bind:this={searchInput}
      type="text"
      placeholder="Filter notes..."
      bind:value={searchTerm}
      oninput={() => draw()}
    />
    {#if searchTerm}
      <button class="clear-btn" onclick={clearSearch} aria-label="Clear filter">&times;</button>
    {/if}
  </div>

  <button class="close-btn" onclick={close} aria-label="Close graph view">&times;</button>
</div>

<style>
  .graph-container {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 10;
    background: var(--bg-primary);
    overflow: hidden;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
  }

  .loading-overlay,
  .empty-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--text-muted);
    font-size: 0.9rem;
    pointer-events: none;
    z-index: 2;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--bg-tertiary);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .tooltip {
    position: absolute;
    transform: translate(-50%, -100%);
    background: var(--bg-secondary);
    color: var(--text-primary);
    border: 1px solid var(--border);
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-family: var(--font-mono);
    white-space: nowrap;
    pointer-events: none;
    z-index: 3;
  }

  .search-box {
    position: absolute;
    top: 12px;
    right: 44px;
    z-index: 5;
    display: flex;
    align-items: center;
  }

  .search-box input {
    width: 180px;
    padding: 5px 28px 5px 10px;
    background: var(--bg-secondary);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 12px;
    font-family: var(--font-mono);
    outline: none;
  }

  .search-box input:focus {
    border-color: var(--accent);
  }

  .search-box input::placeholder {
    color: var(--text-muted);
  }

  .clear-btn {
    position: absolute;
    right: 4px;
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 16px;
    line-height: 1;
    padding: 2px 4px;
  }

  .clear-btn:hover {
    color: var(--text-primary);
  }

  .close-btn {
    position: absolute;
    top: 8px;
    right: 12px;
    z-index: 5;
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 16px;
  }

  .close-btn:hover {
    color: var(--text-primary);
    border-color: var(--accent);
  }
</style>

import {
  SIDEBAR_DEFAULT_WIDTH, SIDE_PANEL_DEFAULT_WIDTH,
  SIDEBAR_MIN_WIDTH, SIDE_PANEL_MIN_WIDTH, EDITOR_MIN_WIDTH,
} from '../ts/constants';

export type SidePanelTab = 'outline' | 'backlinks' | 'tags';

let sidebarWidth = $state(SIDEBAR_DEFAULT_WIDTH);
let sidePanelWidth = $state(SIDE_PANEL_DEFAULT_WIDTH);
let sidebarVisible = $state(true);
let sidePanelVisible = $state(false);
let sidePanelTab = $state<SidePanelTab>('backlinks');
let commandPaletteOpen = $state(false);

export function getUiStore() {
  return {
    get sidebarWidth() { return sidebarWidth; },
    get sidePanelWidth() { return sidePanelWidth; },
    get sidebarVisible() { return sidebarVisible; },
    get sidePanelVisible() { return sidePanelVisible; },
    get sidePanelTab() { return sidePanelTab; },
    get commandPaletteOpen() { return commandPaletteOpen; },
    setSidebarWidth(w: number) { sidebarWidth = Math.max(SIDEBAR_MIN_WIDTH, w); },
    setSidePanelWidth(w: number) { sidePanelWidth = Math.max(SIDE_PANEL_MIN_WIDTH, w); },
    toggleSidebar() { sidebarVisible = !sidebarVisible; },
    toggleSidePanel() { sidePanelVisible = !sidePanelVisible; },
    setSidePanelTab(tab: SidePanelTab) { sidePanelTab = tab; sidePanelVisible = true; },
    setCommandPaletteOpen(v: boolean) { commandPaletteOpen = v; },
  };
}

let _collapsed = $state(true);

export function isSidebarCollapsed() {
  return _collapsed;
}

export function toggleSidebar() {
  _collapsed = !_collapsed;
}

export function collapseSidebar() {
  _collapsed = true;
}

export function expandSidebar() {
  _collapsed = false;
}

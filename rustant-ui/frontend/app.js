// Rustant Dashboard — Main Application
// WebSocket client, page routing, and data refresh logic.

const App = {
  ws: null,
  gatewayPort: 18790,
  refreshInterval: null,
  currentPage: 'dashboard',

  init() {
    this.setupNavigation();
    this.connectWebSocket();
    this.startPolling();
    this.navigateTo('dashboard');
  },

  // --- Navigation ---

  setupNavigation() {
    document.querySelectorAll('.nav-link').forEach(link => {
      link.addEventListener('click', (e) => {
        e.preventDefault();
        const page = link.dataset.page;
        this.navigateTo(page);
      });
    });

    // Handle browser back/forward
    window.addEventListener('hashchange', () => {
      const page = location.hash.slice(1) || 'dashboard';
      this.navigateTo(page, false);
    });
  },

  navigateTo(page, updateHash = true) {
    this.currentPage = page;

    // Update active nav link
    document.querySelectorAll('.nav-link').forEach(l => l.classList.remove('active'));
    const activeLink = document.querySelector(`[data-page="${page}"]`);
    if (activeLink) activeLink.classList.add('active');

    // Show active page
    document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));
    const activePage = document.getElementById(`page-${page}`);
    if (activePage) activePage.classList.add('active');

    if (updateHash) location.hash = page;

    // Refresh page data
    this.refreshCurrentPage();
  },

  // --- WebSocket ---

  connectWebSocket() {
    this.updateWsStatus('connecting');

    try {
      this.ws = new WebSocket(`ws://127.0.0.1:${this.gatewayPort}/ws`);

      this.ws.onopen = () => {
        this.updateWsStatus('connected');
        console.log('WebSocket connected');
      };

      this.ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data);
          this.handleWsMessage(msg);
        } catch (e) {
          console.warn('Failed to parse WS message:', e);
        }
      };

      this.ws.onclose = () => {
        this.updateWsStatus('disconnected');
        console.log('WebSocket disconnected, reconnecting in 3s...');
        setTimeout(() => this.connectWebSocket(), 3000);
      };

      this.ws.onerror = () => {
        this.updateWsStatus('disconnected');
      };
    } catch (e) {
      this.updateWsStatus('disconnected');
      setTimeout(() => this.connectWebSocket(), 3000);
    }
  },

  updateWsStatus(status) {
    const el = document.getElementById('ws-status');
    if (!el) return;
    const dot = el.querySelector('.dot');
    const text = el.querySelector('span:last-child');
    dot.className = `dot ${status}`;
    const labels = { connected: 'Connected', disconnected: 'Disconnected', connecting: 'Connecting...' };
    text.textContent = labels[status] || status;
  },

  handleWsMessage(msg) {
    // Route gateway events to the appropriate page handler
    if (msg.type === 'Event' && msg.event) {
      const event = msg.event;
      DashboardPage.handleEvent(event);
      SessionsPage.handleEvent(event);
      MonitoringPage.handleEvent(event);
      SecurityPage.handleEvent(event);
    }
  },

  // --- REST API Polling ---

  startPolling() {
    this.refreshCurrentPage();
    this.refreshInterval = setInterval(() => this.refreshCurrentPage(), 5000);
  },

  async refreshCurrentPage() {
    switch (this.currentPage) {
      case 'dashboard':
        await DashboardPage.refresh();
        break;
      case 'sessions':
        await SessionsPage.refresh();
        break;
      case 'config':
        await ConfigPage.refresh();
        break;
      case 'monitoring':
        await MonitoringPage.refresh();
        break;
      case 'security':
        await SecurityPage.refresh();
        break;
    }
  },

  // --- API Helpers ---

  async apiGet(path) {
    try {
      const resp = await fetch(`http://127.0.0.1:${this.gatewayPort}${path}`);
      if (!resp.ok) return null;
      return await resp.json();
    } catch (e) {
      console.warn(`API GET ${path} failed:`, e.message);
      return null;
    }
  },

  async apiPost(path, body) {
    try {
      const resp = await fetch(`http://127.0.0.1:${this.gatewayPort}${path}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      if (!resp.ok) return null;
      return await resp.json();
    } catch (e) {
      console.warn(`API POST ${path} failed:`, e.message);
      return null;
    }
  },

  // --- Utilities ---

  formatUptime(secs) {
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${h}h ${m}m`;
  },

  formatTimestamp(iso) {
    if (!iso) return '-';
    const d = new Date(iso);
    return d.toLocaleTimeString();
  },

  escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
  }
};

// Initialize on DOM ready
document.addEventListener('DOMContentLoaded', () => App.init());

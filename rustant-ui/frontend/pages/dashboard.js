// Rustant Dashboard — Dashboard Page
// Agent status, sessions, channels, nodes, activity feed.

const DashboardPage = {
  activityLog: [],

  async refresh() {
    const status = await App.apiGet('/api/status');
    if (!status) {
      this.renderOffline();
      return;
    }

    const el = document.getElementById('page-dashboard');
    el.innerHTML = `
      <div class="page-header">
        <h2>Dashboard</h2>
        <p>Agent overview and real-time activity</p>
      </div>

      <div class="card-grid">
        <div class="card">
          <div class="card-label">Uptime</div>
          <div class="card-value">${App.formatUptime(status.uptime_secs || 0)}</div>
        </div>
        <div class="card">
          <div class="card-label">Active Sessions</div>
          <div class="card-value">${status.active_sessions || 0}</div>
        </div>
        <div class="card">
          <div class="card-label">Connections</div>
          <div class="card-value">${status.active_connections || 0}</div>
        </div>
        <div class="card">
          <div class="card-label">Tool Calls</div>
          <div class="card-value">${status.total_tool_calls || 0}</div>
        </div>
        <div class="card">
          <div class="card-label">LLM Requests</div>
          <div class="card-value">${status.total_llm_requests || 0}</div>
        </div>
        <div class="card">
          <div class="card-label">Version</div>
          <div class="card-value" style="font-size:18px">${App.escapeHtml(status.version || '0.1.0')}</div>
        </div>
      </div>

      <div class="two-col">
        <div class="section">
          <div class="section-title">Channels</div>
          ${this.renderChannels(status.channels || [])}
        </div>
        <div class="section">
          <div class="section-title">Nodes</div>
          ${this.renderNodes(status.nodes || [])}
        </div>
      </div>

      <div class="section">
        <div class="section-title">Activity Feed</div>
        <div class="activity-feed" id="activity-feed">
          ${this.renderActivityLog()}
        </div>
      </div>
    `;
  },

  renderOffline() {
    const el = document.getElementById('page-dashboard');
    el.innerHTML = `
      <div class="page-header">
        <h2>Dashboard</h2>
        <p>Agent overview and real-time activity</p>
      </div>
      <div class="empty-state">
        <p>Gateway is not reachable. Waiting for connection...</p>
      </div>
    `;
  },

  renderChannels(channels) {
    if (!channels || channels.length === 0) {
      return '<div class="card"><div class="empty-state"><p>No channels configured</p></div></div>';
    }
    let html = '<div class="card"><table class="data-table"><thead><tr><th>Channel</th><th>Status</th></tr></thead><tbody>';
    for (const ch of channels) {
      const name = App.escapeHtml(ch.name || ch[0] || '');
      const st = ch.status || ch[1] || 'unknown';
      const badge = st === 'connected' ? 'badge-success' : 'badge-warning';
      html += `<tr><td>${name}</td><td><span class="badge ${badge}">${App.escapeHtml(st)}</span></td></tr>`;
    }
    html += '</tbody></table></div>';
    return html;
  },

  renderNodes(nodes) {
    if (!nodes || nodes.length === 0) {
      return '<div class="card"><div class="empty-state"><p>No nodes registered</p></div></div>';
    }
    let html = '<div class="card"><table class="data-table"><thead><tr><th>Node</th><th>Status</th></tr></thead><tbody>';
    for (const n of nodes) {
      const name = App.escapeHtml(n.name || n[0] || '');
      const st = n.status || n[1] || 'unknown';
      const badge = st === 'healthy' ? 'badge-success' : 'badge-warning';
      html += `<tr><td>${name}</td><td><span class="badge ${badge}">${App.escapeHtml(st)}</span></td></tr>`;
    }
    html += '</tbody></table></div>';
    return html;
  },

  renderActivityLog() {
    if (this.activityLog.length === 0) {
      return '<div class="empty-state"><p>No activity yet</p></div>';
    }
    return this.activityLog.slice(-50).reverse().map(item => `
      <div class="activity-item">
        <span class="activity-time">${item.time}</span>
        <span class="activity-text">${App.escapeHtml(item.text)}</span>
      </div>
    `).join('');
  },

  handleEvent(event) {
    if (!event || !event.type) return;
    const now = new Date().toLocaleTimeString();
    let text = '';

    switch (event.type) {
      case 'Connected': text = `Client connected: ${event.connection_id}`; break;
      case 'Disconnected': text = `Client disconnected: ${event.connection_id}`; break;
      case 'TaskSubmitted': text = `Task submitted: ${event.description}`; break;
      case 'TaskProgress': text = `Task progress: ${event.message} (${Math.round(event.progress * 100)}%)`; break;
      case 'TaskCompleted': text = `Task completed: ${event.summary}`; break;
      case 'ToolExecution': text = `Tool: ${event.tool_name} [${event.status}]`; break;
      case 'Error': text = `Error: ${event.message}`; break;
      default: text = `Event: ${event.type}`; break;
    }

    if (text) {
      this.activityLog.push({ time: now, text });
      if (this.activityLog.length > 200) this.activityLog.shift();
    }
  }
};

// Rustant Dashboard — Sessions Page
// Session list, search, details.

const SessionsPage = {
  sessions: [],

  async refresh() {
    const data = await App.apiGet('/api/sessions');
    if (data) this.sessions = data.sessions || [];
    this.render();
  },

  render() {
    const el = document.getElementById('page-sessions');
    el.innerHTML = `
      <div class="page-header">
        <h2>Sessions</h2>
        <p>Active and recent agent sessions</p>
      </div>

      <div class="section">
        <div class="card">
          ${this.sessions.length === 0
            ? '<div class="empty-state"><p>No sessions</p></div>'
            : this.renderTable()
          }
        </div>
      </div>
    `;
  },

  renderTable() {
    let html = `
      <table class="data-table">
        <thead>
          <tr>
            <th>Session ID</th>
            <th>State</th>
            <th>Created</th>
          </tr>
        </thead>
        <tbody>
    `;
    for (const s of this.sessions) {
      const id = App.escapeHtml((s.id || '').slice(0, 8) + '...');
      const state = s.state || 'unknown';
      const badge = state === 'Active' ? 'badge-success' : state === 'Paused' ? 'badge-warning' : 'badge-info';
      const created = App.formatTimestamp(s.created_at);
      html += `<tr>
        <td title="${App.escapeHtml(s.id || '')}">${id}</td>
        <td><span class="badge ${badge}">${App.escapeHtml(state)}</span></td>
        <td>${created}</td>
      </tr>`;
    }
    html += '</tbody></table>';
    return html;
  },

  handleEvent(event) {
    // Sessions page can react to session-related events if needed
  }
};

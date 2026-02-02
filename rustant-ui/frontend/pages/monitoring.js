// Rustant Dashboard — Monitoring Page
// Real-time metrics, tool call and LLM request counts.

const MonitoringPage = {
  metricsHistory: [],

  async refresh() {
    const data = await App.apiGet('/api/metrics');
    if (data) {
      this.metricsHistory.push({
        time: new Date().toLocaleTimeString(),
        ...data,
      });
      if (this.metricsHistory.length > 60) this.metricsHistory.shift();
    }
    this.render(data);
  },

  render(data) {
    const el = document.getElementById('page-monitoring');
    if (!data) {
      el.innerHTML = `
        <div class="page-header">
          <h2>Monitoring</h2>
          <p>Real-time performance metrics</p>
        </div>
        <div class="empty-state"><p>Waiting for metrics data...</p></div>
      `;
      return;
    }

    el.innerHTML = `
      <div class="page-header">
        <h2>Monitoring</h2>
        <p>Real-time performance metrics</p>
      </div>

      <div class="card-grid">
        <div class="card">
          <div class="card-label">Uptime</div>
          <div class="card-value">${App.formatUptime(data.uptime_secs || 0)}</div>
        </div>
        <div class="card">
          <div class="card-label">Total Tool Calls</div>
          <div class="card-value">${data.total_tool_calls || 0}</div>
        </div>
        <div class="card">
          <div class="card-label">Total LLM Requests</div>
          <div class="card-value">${data.total_llm_requests || 0}</div>
        </div>
        <div class="card">
          <div class="card-label">Active Connections</div>
          <div class="card-value">${data.active_connections || 0}</div>
        </div>
        <div class="card">
          <div class="card-label">Active Sessions</div>
          <div class="card-value">${data.active_sessions || 0}</div>
        </div>
      </div>

      <div class="section">
        <div class="section-title">Metrics History (last ${this.metricsHistory.length} samples)</div>
        <div class="card">
          ${this.renderHistory()}
        </div>
      </div>
    `;
  },

  renderHistory() {
    if (this.metricsHistory.length === 0) {
      return '<div class="empty-state"><p>No history yet</p></div>';
    }
    let html = `
      <table class="data-table">
        <thead><tr>
          <th>Time</th><th>Tool Calls</th><th>LLM Requests</th><th>Connections</th><th>Sessions</th>
        </tr></thead>
        <tbody>
    `;
    for (const m of this.metricsHistory.slice(-20).reverse()) {
      html += `<tr>
        <td>${m.time}</td>
        <td>${m.total_tool_calls || 0}</td>
        <td>${m.total_llm_requests || 0}</td>
        <td>${m.active_connections || 0}</td>
        <td>${m.active_sessions || 0}</td>
      </tr>`;
    }
    html += '</tbody></table>';
    return html;
  },

  handleEvent(event) {
    if (event.type === 'MetricsUpdate') {
      this.metricsHistory.push({
        time: new Date().toLocaleTimeString(),
        ...event,
      });
      if (this.metricsHistory.length > 60) this.metricsHistory.shift();
    }
  }
};

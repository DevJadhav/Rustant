// Rustant Dashboard — Security Page
// Approval queue, audit log viewer.

const SecurityPage = {
  approvals: [],
  auditEntries: [],

  async refresh() {
    const approvalsData = await App.apiGet('/api/approvals');
    if (approvalsData) this.approvals = approvalsData.approvals || [];

    const auditData = await App.apiGet('/api/audit');
    if (auditData) this.auditEntries = auditData.entries || [];

    this.render();
  },

  render() {
    const el = document.getElementById('page-security');
    el.innerHTML = `
      <div class="page-header">
        <h2>Security</h2>
        <p>Approval queue and audit trail</p>
      </div>

      <div class="section">
        <div class="section-title">Pending Approvals (${this.approvals.length})</div>
        ${this.approvals.length === 0
          ? '<div class="card"><div class="empty-state"><p>No pending approvals</p></div></div>'
          : this.renderApprovals()
        }
      </div>

      <div class="section">
        <div class="section-title">Audit Log</div>
        ${this.auditEntries.length === 0
          ? '<div class="card"><div class="empty-state"><p>No audit entries</p></div></div>'
          : this.renderAudit()
        }
      </div>
    `;

    // Attach button handlers
    el.querySelectorAll('.btn-approve').forEach(btn => {
      btn.addEventListener('click', () => this.handleApproval(btn.dataset.id, true));
    });
    el.querySelectorAll('.btn-deny').forEach(btn => {
      btn.addEventListener('click', () => this.handleApproval(btn.dataset.id, false));
    });
  },

  renderApprovals() {
    return this.approvals.map(a => {
      const riskBadge = a.risk_level === 'high' ? 'badge-danger'
        : a.risk_level === 'medium' ? 'badge-warning' : 'badge-info';
      return `
        <div class="approval-card">
          <div class="tool-name">${App.escapeHtml(a.tool_name)}</div>
          <div class="description">${App.escapeHtml(a.description)}</div>
          <span class="badge ${riskBadge}">${App.escapeHtml(a.risk_level)}</span>
          <div class="approval-actions" style="margin-top:12px">
            <button class="btn btn-success btn-approve" data-id="${App.escapeHtml(a.id)}">Approve</button>
            <button class="btn btn-danger btn-deny" data-id="${App.escapeHtml(a.id)}">Deny</button>
          </div>
        </div>
      `;
    }).join('');
  },

  renderAudit() {
    let html = `
      <div class="card">
        <table class="data-table">
          <thead><tr><th>Time</th><th>Action</th><th>Details</th></tr></thead>
          <tbody>
    `;
    for (const entry of this.auditEntries.slice(-50).reverse()) {
      html += `<tr>
        <td>${App.formatTimestamp(entry.timestamp)}</td>
        <td>${App.escapeHtml(entry.action || '')}</td>
        <td>${App.escapeHtml(entry.details || '')}</td>
      </tr>`;
    }
    html += '</tbody></table></div>';
    return html;
  },

  async handleApproval(id, approved) {
    const result = await App.apiPost(`/api/approval/${id}`, { approved });
    if (result) {
      // Remove from local list and re-render
      this.approvals = this.approvals.filter(a => a.id !== id);
      this.render();
    }
  },

  handleEvent(event) {
    if (event.type === 'ApprovalRequest') {
      this.approvals.push({
        id: event.approval_id,
        tool_name: event.tool_name,
        description: event.description,
        risk_level: event.risk_level,
      });
    }
  }
};

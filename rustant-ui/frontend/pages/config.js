// Rustant Dashboard — Configuration Page
// Visual config editor, current configuration display.

const ConfigPage = {
  configData: null,

  async refresh() {
    const data = await App.apiGet('/api/config');
    if (data) this.configData = data;
    this.render();
  },

  render() {
    const el = document.getElementById('page-config');
    const prettyJson = this.configData
      ? JSON.stringify(this.configData, null, 2)
      : '{}';

    el.innerHTML = `
      <div class="page-header">
        <h2>Configuration</h2>
        <p>Current agent configuration (read-only)</p>
      </div>

      <div class="section">
        <div class="config-editor">
          <pre>${App.escapeHtml(prettyJson)}</pre>
        </div>
      </div>
    `;
  }
};

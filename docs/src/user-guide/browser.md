# Browser Automation

Rustant includes headless browser automation via the Chrome DevTools Protocol (CDP).

## Prerequisites

A Chromium-based browser (Chrome, Chromium, Edge) must be installed. Rustant will auto-detect the browser location on macOS, Linux, and Windows.

## Testing

```bash
rustant browser test https://example.com
```

This launches a headless browser, navigates to the URL, prints the page title, URL, and a text preview, takes a screenshot, and closes the browser.

## Configuration

```toml
[browser]
enabled = true
headless = true
timeout_secs = 30
viewport_width = 1280
viewport_height = 720
```

## CDP Client

The browser automation is built on a CDP client that supports:

- **Navigation** — `navigate(url)`, `wait_for_load()`
- **Content extraction** — `get_title()`, `get_url()`, `get_text()`, `get_html()`
- **Screenshots** — `screenshot()` returns PNG bytes
- **JavaScript execution** — `evaluate(expression)`
- **Element interaction** — `click(selector)`, `type_text(selector, text)`

## Security

Browser sessions run in a security sandbox:

- Navigation is restricted to allowed domains (configurable)
- JavaScript execution is logged in the audit trail
- Download operations require explicit approval
- Cookie and credential access is controlled by the safety guardian

## Build Feature

Browser support is behind the `browser` feature flag. To enable it:

```bash
cargo build --features browser
```

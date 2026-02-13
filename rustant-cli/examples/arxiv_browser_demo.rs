//! End-to-end browser automation demo: Search Google for arXiv, navigate to arXiv,
//! search for a specific paper, and download the PDF to ~/Downloads.
//!
//! Run with:
//!   cargo run --example arxiv_browser_demo --features browser

#[cfg(feature = "browser")]
mod demo {
    use rustant_core::browser::{CdpClient, ChromiumCdpClient};
    use rustant_core::config::BrowserConfig;
    use std::path::PathBuf;

    const SCREENSHOT_DIR: &str = "/Users/dev/Downloads";

    async fn save_screenshot(client: &ChromiumCdpClient, stage: &str, step: usize) -> PathBuf {
        let filename = format!("rustant_stage{}_{}.png", step, stage);
        let path = PathBuf::from(SCREENSHOT_DIR).join(&filename);
        match client.screenshot().await {
            Ok(bytes) => {
                std::fs::write(&path, &bytes).expect("Failed to write screenshot");
                println!(
                    "  [Screenshot] {} ({} bytes) -> {}",
                    stage,
                    bytes.len(),
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("  [Screenshot FAILED] {}: {}", stage, e);
            }
        }
        path
    }

    async fn wait_ms(ms: u64) {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
    }

    pub async fn run() -> anyhow::Result<()> {
        println!("==========================================================");
        println!("  Rustant Browser Automation Demo");
        println!("  Task: Search Google -> arXiv -> Find & Download Paper");
        println!("==========================================================\n");

        // --- STAGE 0: Launch Chrome (non-headless so we can see it) ---
        println!("[Stage 0] Launching Chrome...");
        let config = BrowserConfig {
            enabled: true,
            headless: false,
            default_viewport_width: 1440,
            default_viewport_height: 900,
            default_timeout_secs: 30,
            ..Default::default()
        };
        let client = ChromiumCdpClient::launch(&config).await?;
        println!("  Chrome launched successfully.\n");

        // --- STAGE 1: Search Google for "arxiv" ---
        println!("[Stage 1] Navigating to Google...");
        client.navigate("https://www.google.com").await?;
        wait_ms(2000).await;
        let title = client.get_title().await.unwrap_or_default();
        println!("  Page title: {}", title);
        save_screenshot(&client, "google_homepage", 1).await;

        println!("\n[Stage 1b] Typing 'arxiv' into Google search...");
        // Google search box selectors
        let search_typed = client
            .evaluate_js(
                r#"
            (function() {
                let el = document.querySelector('textarea[name="q"]')
                      || document.querySelector('input[name="q"]');
                if (el) {
                    el.focus();
                    el.value = 'arxiv';
                    el.dispatchEvent(new Event('input', {bubbles: true}));
                    return 'ok';
                }
                return 'not_found';
            })()
        "#,
            )
            .await?;
        println!("  Search box fill: {:?}", search_typed);
        wait_ms(1000).await;
        save_screenshot(&client, "google_typed_arxiv", 2).await;

        println!("\n[Stage 1c] Submitting Google search...");
        client.evaluate_js(r#"
            (function() {
                let form = document.querySelector('form[action="/search"]');
                if (form) { form.submit(); return 'submitted'; }
                let el = document.querySelector('textarea[name="q"]')
                      || document.querySelector('input[name="q"]');
                if (el) {
                    el.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', code:'Enter', keyCode:13, which:13, bubbles:true}));
                    return 'enter_pressed';
                }
                return 'failed';
            })()
        "#).await?;
        wait_ms(3000).await;
        let title = client.get_title().await.unwrap_or_default();
        println!("  Search results page: {}", title);
        save_screenshot(&client, "google_search_results", 3).await;

        // --- STAGE 2: Click the arXiv link from Google results ---
        println!("\n[Stage 2] Looking for arXiv link in search results...");
        let click_result = client.evaluate_js(r#"
            (function() {
                let links = document.querySelectorAll('a');
                for (let a of links) {
                    let href = a.getAttribute('href') || '';
                    let text = a.textContent || '';
                    if (href.includes('arxiv.org') && !href.includes('google') && text.toLowerCase().includes('arxiv')) {
                        a.click();
                        return 'clicked: ' + href;
                    }
                }
                return 'not_found';
            })()
        "#).await?;
        println!("  arXiv link: {:?}", click_result);
        wait_ms(3000).await;

        let url = client.get_url().await.unwrap_or_default();
        println!("  Current URL: {}", url);

        // If we didn't reach arxiv.org, navigate directly
        if !url.contains("arxiv.org") {
            println!("  Navigating directly to arxiv.org...");
            client.navigate("https://arxiv.org").await?;
            wait_ms(3000).await;
        }

        let title = client.get_title().await.unwrap_or_default();
        println!("  arXiv page title: {}", title);
        save_screenshot(&client, "arxiv_homepage", 4).await;

        // --- STAGE 3: Search for the specific paper on arXiv ---
        println!("\n[Stage 3] Searching arXiv for the paper...");
        let paper_query = "Reinforcement Learning via Self-Distillation Hubotter";

        // Use arXiv search with author name for precise matching
        let search_url = format!(
            "https://arxiv.org/search/?searchtype=all&query={}",
            paper_query.replace(' ', "+")
        );
        client.navigate(&search_url).await?;
        wait_ms(3000).await;
        let title = client.get_title().await.unwrap_or_default();
        println!("  Search results: {}", title);
        save_screenshot(&client, "arxiv_search_results", 5).await;

        // --- STAGE 4: Find and click the specific paper ---
        println!("\n[Stage 4] Looking for the specific paper by Hübotter et al...");
        let paper_link = client
            .evaluate_js(
                r#"
            (function() {
                // Look for the paper by checking all arxiv-result items
                let results = document.querySelectorAll('li.arxiv-result');
                for (let r of results) {
                    let title = r.querySelector('p.title');
                    let authors = r.querySelector('p.authors');
                    let titleText = title ? title.textContent.toLowerCase() : '';
                    let authorText = authors ? authors.textContent.toLowerCase() : '';
                    // Match on "reinforcement learning" + "self-distillation" + author "botter"
                    if (titleText.includes('reinforcement') &&
                        titleText.includes('self-distill') &&
                        authorText.includes('botter')) {
                        let link = r.querySelector('a[href*="/abs/"]');
                        if (link) {
                            return 'found: ' + link.href;
                        }
                    }
                }
                // Broader search: just title matching
                for (let r of results) {
                    let title = r.querySelector('p.title');
                    let titleText = title ? title.textContent.toLowerCase() : '';
                    if (titleText.includes('reinforcement') && titleText.includes('self-distill')) {
                        let link = r.querySelector('a[href*="/abs/"]');
                        if (link) {
                            return 'found_broad: ' + link.href;
                        }
                    }
                }
                return 'not_found';
            })()
        "#,
            )
            .await?;
        println!("  Paper link result: {:?}", paper_link);

        // Extract the URL from the result
        let paper_url = paper_link.as_str().unwrap_or("not_found").to_string();
        if paper_url.contains("/abs/") {
            let abs_url = if paper_url.contains("http") {
                paper_url
                    .split("found: ")
                    .last()
                    .or_else(|| paper_url.split("found_broad: ").last())
                    .unwrap_or(&paper_url)
                    .to_string()
            } else {
                paper_url.clone()
            };
            println!("  Navigating to paper: {}", abs_url);
            client.navigate(&abs_url).await?;
            wait_ms(3000).await;
        } else {
            // Try refined search with more specific terms
            println!("  Paper not found in first search, trying refined search...");
            client.navigate("https://arxiv.org/search/?searchtype=all&query=%22Reinforcement+Learning+via+Self-Distillation%22").await?;
            wait_ms(3000).await;
            save_screenshot(&client, "arxiv_search_refined", 6).await;

            // Try to find it with exact title match
            let paper_link2 = client
                .evaluate_js(
                    r#"
                (function() {
                    let results = document.querySelectorAll('li.arxiv-result');
                    for (let r of results) {
                        let title = r.querySelector('p.title');
                        let titleText = title ? title.textContent.trim().toLowerCase() : '';
                        if (titleText.includes('reinforcement learning via self-distillation')) {
                            let link = r.querySelector('a[href*="/abs/"]');
                            if (link) return link.href;
                        }
                    }
                    return 'not_found';
                })()
            "#,
                )
                .await?;
            println!("  Refined search result: {:?}", paper_link2);

            if let Some(href) = paper_link2.as_str() {
                if href.contains("/abs/") {
                    let abs_url = if href.starts_with("http") {
                        href.to_string()
                    } else {
                        format!("https://arxiv.org{}", href)
                    };
                    client.navigate(&abs_url).await?;
                    wait_ms(3000).await;
                }
            }
        }

        let url = client.get_url().await.unwrap_or_default();
        let title = client.get_title().await.unwrap_or_default();
        println!("  Paper page - Title: {}", title);
        println!("  Paper page - URL: {}", url);
        save_screenshot(&client, "arxiv_paper_page", 7).await;

        // --- STAGE 5: Navigate to PDF and download ---
        println!("\n[Stage 5] Finding and downloading PDF...");

        // Extract the paper ID from the URL
        let paper_id = if url.contains("/abs/") {
            url.split("/abs/")
                .last()
                .unwrap_or("")
                .trim_matches('/')
                .to_string()
        } else {
            // Try to find from page content
            let id_result = client
                .evaluate_js(
                    r#"
                (function() {
                    let links = document.querySelectorAll('a');
                    for (let a of links) {
                        let href = a.getAttribute('href') || '';
                        if (href.includes('/pdf/')) {
                            return href;
                        }
                    }
                    return 'not_found';
                })()
            "#,
                )
                .await?;
            id_result.as_str().unwrap_or("").to_string()
        };
        println!("  Paper ID: {}", paper_id);

        // Construct the PDF URL
        let pdf_url = if paper_id.contains("/pdf/") {
            if paper_id.starts_with("http") {
                paper_id.clone()
            } else {
                format!("https://arxiv.org{}", paper_id)
            }
        } else if !paper_id.is_empty() && paper_id != "not_found" {
            format!("https://arxiv.org/pdf/{}", paper_id)
        } else {
            // Fallback: look for the PDF link on the page
            let pdf_href = client
                .evaluate_js(
                    r#"
                (function() {
                    let links = document.querySelectorAll('a');
                    for (let a of links) {
                        let href = a.getAttribute('href') || '';
                        let text = (a.textContent || '').toLowerCase();
                        if (href.includes('/pdf/') || text.includes('pdf')) {
                            if (href.startsWith('http')) return href;
                            return 'https://arxiv.org' + href;
                        }
                    }
                    return '';
                })()
            "#,
                )
                .await?;
            pdf_href.as_str().unwrap_or("").to_string()
        };

        println!("  PDF URL: {}", pdf_url);

        if pdf_url.is_empty() {
            println!("  ERROR: Could not determine PDF URL. Trying alternative approach...");
            // Navigate to search and try to find it
            client.navigate("https://arxiv.org/search/?searchtype=all&query=self-distillation+reinforcement+learning+hubotter+2026").await?;
            wait_ms(3000).await;
            save_screenshot(&client, "arxiv_alt_search", 8).await;
        }

        // Download PDF using reqwest (more reliable than browser download)
        if !pdf_url.is_empty() {
            println!("  Navigating to PDF page for screenshot...");
            client.navigate(&pdf_url).await?;
            wait_ms(5000).await;
            save_screenshot(&client, "arxiv_pdf_view", 8).await;

            println!("  Downloading PDF via HTTP...");
            let download_path = PathBuf::from(SCREENSHOT_DIR)
                .join("Reinforcement_Learning_via_Self_Distillation.pdf");

            let http_client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Rustant/1.0")
                .build()?;

            let response = http_client.get(&pdf_url).send().await?;

            if response.status().is_success() {
                let bytes = response.bytes().await?;
                std::fs::write(&download_path, &bytes)?;
                println!(
                    "  PDF downloaded successfully: {} ({} bytes)",
                    download_path.display(),
                    bytes.len()
                );
            } else {
                println!("  Download failed with status: {}", response.status());
            }
        }

        // --- STAGE 6: Final verification ---
        println!("\n[Stage 6] Final verification...");
        let download_path =
            PathBuf::from(SCREENSHOT_DIR).join("Reinforcement_Learning_via_Self_Distillation.pdf");
        if download_path.exists() {
            let metadata = std::fs::metadata(&download_path)?;
            println!("  PDF file exists: {}", download_path.display());
            println!(
                "  File size: {} bytes ({:.1} KB)",
                metadata.len(),
                metadata.len() as f64 / 1024.0
            );
            println!("  DOWNLOAD SUCCESSFUL!");
        } else {
            println!("  WARNING: PDF file not found at expected path");
        }

        // Take final screenshot
        save_screenshot(&client, "final_state", 9).await;

        // Cleanup
        println!("\n[Stage 7] Closing browser...");
        client.close().await?;
        println!("  Browser closed.\n");

        println!("==========================================================");
        println!("  Demo Complete!");
        println!("  Screenshots saved to: {}", SCREENSHOT_DIR);
        println!("==========================================================");

        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(feature = "browser")]
    {
        demo::run().await
    }
    #[cfg(not(feature = "browser"))]
    {
        eprintln!("Browser feature not enabled. Recompile with: cargo build --features browser");
        Ok(())
    }
}

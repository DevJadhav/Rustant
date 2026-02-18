//! Multi-source paper API clients and citation graph analysis.
//!
//! Provides unified access to Semantic Scholar and OpenAlex APIs alongside
//! the existing ArXiv client, with in-memory response caching and citation
//! network analysis (PageRank, bibliographic coupling, shortest path).

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ── External Metadata ─────────────────────────────────────────

/// Additional metadata from external sources (Semantic Scholar, OpenAlex).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExternalMetadata {
    #[serde(default)]
    pub citation_count: Option<u64>,
    #[serde(default)]
    pub influential_citation_count: Option<u64>,
    #[serde(default)]
    pub tldr: Option<String>,
    #[serde(default)]
    pub semantic_scholar_id: Option<String>,
    #[serde(default)]
    pub openalex_id: Option<String>,
    #[serde(default)]
    pub concepts: Vec<String>,
    #[serde(default)]
    pub references: Vec<String>,
    #[serde(default)]
    pub cited_by: Vec<String>,
}

// ── API Response Cache ────────────────────────────────────────

struct CacheEntry {
    data: serde_json::Value,
    cached_at: Instant,
}

/// In-memory response cache with TTL and LRU eviction.
pub struct ApiResponseCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
    ttl: Duration,
    max_entries: usize,
}

impl ApiResponseCache {
    pub fn new(ttl_secs: u64, max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
            max_entries,
        }
    }

    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        let entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(key)
            && entry.cached_at.elapsed() < self.ttl
        {
            return Some(entry.data.clone());
        }
        None
    }

    pub fn insert(&self, key: String, data: serde_json::Value) {
        let mut entries = self.entries.lock().unwrap();

        // LRU eviction: remove oldest entry if at capacity
        if entries.len() >= self.max_entries
            && !entries.contains_key(&key)
            && let Some(oldest_key) = entries
                .iter()
                .min_by_key(|(_, v)| v.cached_at)
                .map(|(k, _)| k.clone())
        {
            entries.remove(&oldest_key);
        }

        entries.insert(
            key,
            CacheEntry {
                data,
                cached_at: Instant::now(),
            },
        );
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap().is_empty()
    }
}

// ── Semantic Scholar Client ───────────────────────────────────

const SEMANTIC_SCHOLAR_API: &str = "https://api.semanticscholar.org/graph/v1";
const SEMANTIC_SCHOLAR_FIELDS: &str =
    "paperId,title,abstract,citationCount,influentialCitationCount,tldr,references,citations";

/// Rate-limited Semantic Scholar API client.
pub struct SemanticScholarClient {
    client: reqwest::Client,
    api_key: Option<String>,
    last_request: Mutex<Option<Instant>>,
    cache: ApiResponseCache,
}

impl SemanticScholarClient {
    pub fn new(
        api_key: Option<String>,
        cache_ttl_secs: u64,
        cache_max: usize,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Rustant/1.0")
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
        Ok(Self {
            client,
            api_key,
            last_request: Mutex::new(None),
            cache: ApiResponseCache::new(cache_ttl_secs, cache_max),
        })
    }

    /// Enforce 1-second minimum delay between requests.
    async fn rate_limit(&self) {
        let wait_duration = {
            let last = self.last_request.lock().unwrap();
            if let Some(instant) = *last {
                let elapsed = instant.elapsed();
                if elapsed < Duration::from_secs(1) {
                    Some(Duration::from_secs(1) - elapsed)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(wait) = wait_duration {
            tokio::time::sleep(wait).await;
        }
        let mut last = self.last_request.lock().unwrap();
        *last = Some(Instant::now());
    }

    /// Fetch paper metadata by arXiv ID.
    pub async fn fetch_by_arxiv_id(&self, arxiv_id: &str) -> Result<ExternalMetadata, String> {
        let cache_key = format!("s2:arxiv:{}", arxiv_id);
        if let Some(cached) = self.cache.get(&cache_key) {
            return parse_semantic_scholar_response(&cached);
        }

        self.rate_limit().await;

        let url = format!(
            "{}/paper/ArXiv:{}?fields={}",
            SEMANTIC_SCHOLAR_API, arxiv_id, SEMANTIC_SCHOLAR_FIELDS
        );

        let mut request = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            request = request.header("x-api-key", key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Semantic Scholar request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Semantic Scholar returned status {}",
                response.status()
            ));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Semantic Scholar response: {}", e))?;

        self.cache.insert(cache_key, body.clone());
        parse_semantic_scholar_response(&body)
    }

    /// Fetch citations (papers that cite this paper).
    pub async fn fetch_citations(
        &self,
        arxiv_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, String)>, String> {
        let cache_key = format!("s2:citations:{}", arxiv_id);
        if let Some(cached) = self.cache.get(&cache_key) {
            return parse_citation_list(&cached);
        }

        self.rate_limit().await;

        let url = format!(
            "{}/paper/ArXiv:{}/citations?fields=paperId,title,externalIds&limit={}",
            SEMANTIC_SCHOLAR_API, arxiv_id, limit
        );

        let mut request = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            request = request.header("x-api-key", key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Semantic Scholar citations request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Semantic Scholar citations returned status {}",
                response.status()
            ));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse citations response: {}", e))?;

        self.cache.insert(cache_key, body.clone());
        parse_citation_list(&body)
    }

    /// Fetch references (papers this paper cites).
    pub async fn fetch_references(
        &self,
        arxiv_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, String)>, String> {
        let cache_key = format!("s2:references:{}", arxiv_id);
        if let Some(cached) = self.cache.get(&cache_key) {
            return parse_citation_list(&cached);
        }

        self.rate_limit().await;

        let url = format!(
            "{}/paper/ArXiv:{}/references?fields=paperId,title,externalIds&limit={}",
            SEMANTIC_SCHOLAR_API, arxiv_id, limit
        );

        let mut request = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            request = request.header("x-api-key", key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("Semantic Scholar references request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Semantic Scholar references returned status {}",
                response.status()
            ));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse references response: {}", e))?;

        self.cache.insert(cache_key, body.clone());
        parse_citation_list(&body)
    }
}

fn parse_semantic_scholar_response(body: &serde_json::Value) -> Result<ExternalMetadata, String> {
    let citation_count = body.get("citationCount").and_then(|v| v.as_u64());
    let influential_citation_count = body
        .get("influentialCitationCount")
        .and_then(|v| v.as_u64());
    let tldr = body
        .get("tldr")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let semantic_scholar_id = body
        .get("paperId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let references = body
        .get("references")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    r.get("paperId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    let cited_by = body
        .get("citations")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    r.get("paperId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ExternalMetadata {
        citation_count,
        influential_citation_count,
        tldr,
        semantic_scholar_id,
        openalex_id: None,
        concepts: Vec::new(),
        references,
        cited_by,
    })
}

fn parse_citation_list(body: &serde_json::Value) -> Result<Vec<(String, String)>, String> {
    let data = body
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "No 'data' array in response".to_string())?;

    let mut results = Vec::new();
    for item in data {
        let citing_paper = item
            .get("citingPaper")
            .or_else(|| item.get("citedPaper"))
            .unwrap_or(item);
        let paper_id = citing_paper
            .get("paperId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let title = citing_paper
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        if !paper_id.is_empty() {
            results.push((paper_id, title));
        }
    }
    Ok(results)
}

// ── OpenAlex Client ───────────────────────────────────────────

const OPENALEX_API: &str = "https://api.openalex.org";

/// Rate-limited OpenAlex API client.
pub struct OpenAlexClient {
    client: reqwest::Client,
    email: Option<String>,
    last_request: Mutex<Option<Instant>>,
    cache: ApiResponseCache,
}

impl OpenAlexClient {
    pub fn new(
        email: Option<String>,
        cache_ttl_secs: u64,
        cache_max: usize,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Rustant/1.0")
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
        Ok(Self {
            client,
            email,
            last_request: Mutex::new(None),
            cache: ApiResponseCache::new(cache_ttl_secs, cache_max),
        })
    }

    /// Enforce 100ms minimum delay between requests.
    async fn rate_limit(&self) {
        let wait_duration = {
            let last = self.last_request.lock().unwrap();
            if let Some(instant) = *last {
                let elapsed = instant.elapsed();
                let min_delay = Duration::from_millis(100);
                if elapsed < min_delay {
                    Some(min_delay - elapsed)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(wait) = wait_duration {
            tokio::time::sleep(wait).await;
        }
        let mut last = self.last_request.lock().unwrap();
        *last = Some(Instant::now());
    }

    /// Fetch paper metadata by arXiv ID.
    pub async fn fetch_by_arxiv_id(&self, arxiv_id: &str) -> Result<ExternalMetadata, String> {
        let cache_key = format!("oa:arxiv:{}", arxiv_id);
        if let Some(cached) = self.cache.get(&cache_key) {
            return parse_openalex_response(&cached);
        }

        self.rate_limit().await;

        let mut url = format!(
            "{}/works?filter=ids.openalex:https://openalex.org/W*&filter=doi:https://doi.org/10.48550/arXiv.{}",
            OPENALEX_API, arxiv_id
        );
        // Try direct filter approach
        url = format!(
            "{}/works?filter=ids.openalex_id:*&per_page=1&search=arXiv:{}",
            OPENALEX_API, arxiv_id
        );

        if let Some(ref email) = self.email {
            url.push_str(&format!("&mailto={}", email));
        }

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("OpenAlex request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("OpenAlex returned status {}", response.status()));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAlex response: {}", e))?;

        self.cache.insert(cache_key, body.clone());
        parse_openalex_response(&body)
    }
}

fn parse_openalex_response(body: &serde_json::Value) -> Result<ExternalMetadata, String> {
    // OpenAlex returns results in a "results" array
    let work = body
        .get("results")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .unwrap_or(body);

    let citation_count = work.get("cited_by_count").and_then(|v| v.as_u64());
    let openalex_id = work
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let concepts = work
        .get("concepts")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    c.get("display_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ExternalMetadata {
        citation_count,
        influential_citation_count: None,
        tldr: None,
        semantic_scholar_id: None,
        openalex_id,
        concepts,
        references: Vec::new(),
        cited_by: Vec::new(),
    })
}

// ── Citation Graph ────────────────────────────────────────────

/// Serializable citation graph state for persistence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CitationGraphState {
    pub seed_paper: String,
    #[serde(default)]
    pub references: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub cited_by: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub titles: HashMap<String, String>,
    #[serde(default)]
    pub citation_counts: HashMap<String, u64>,
    #[serde(default)]
    pub built_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// In-memory citation graph for analysis.
pub struct CitationGraph {
    pub references: HashMap<String, HashSet<String>>,
    pub cited_by: HashMap<String, HashSet<String>>,
    pub titles: HashMap<String, String>,
    pub citation_counts: HashMap<String, u64>,
}

impl Default for CitationGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CitationGraph {
    pub fn new() -> Self {
        Self {
            references: HashMap::new(),
            cited_by: HashMap::new(),
            titles: HashMap::new(),
            citation_counts: HashMap::new(),
        }
    }

    /// Restore from persisted state.
    pub fn from_state(state: &CitationGraphState) -> Self {
        let mut graph = Self::new();
        graph.titles = state.titles.clone();
        graph.citation_counts = state.citation_counts.clone();
        for (paper, refs) in &state.references {
            graph
                .references
                .insert(paper.clone(), refs.iter().cloned().collect());
        }
        for (paper, citers) in &state.cited_by {
            graph
                .cited_by
                .insert(paper.clone(), citers.iter().cloned().collect());
        }
        graph
    }

    /// Convert to serializable state.
    pub fn to_state(&self, seed_paper: &str) -> CitationGraphState {
        CitationGraphState {
            seed_paper: seed_paper.to_string(),
            references: self
                .references
                .iter()
                .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
                .collect(),
            cited_by: self
                .cited_by
                .iter()
                .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
                .collect(),
            titles: self.titles.clone(),
            citation_counts: self.citation_counts.clone(),
            built_at: Some(chrono::Utc::now()),
        }
    }

    /// Add a paper and its relationships to the graph.
    pub fn add_paper(
        &mut self,
        paper_id: &str,
        title: &str,
        citation_count: u64,
        refs: Vec<String>,
        citers: Vec<String>,
    ) {
        self.titles.insert(paper_id.to_string(), title.to_string());
        self.citation_counts
            .insert(paper_id.to_string(), citation_count);

        let ref_set: HashSet<String> = refs.into_iter().collect();
        for r in &ref_set {
            self.cited_by
                .entry(r.clone())
                .or_default()
                .insert(paper_id.to_string());
        }
        self.references.insert(paper_id.to_string(), ref_set);

        let citer_set: HashSet<String> = citers.into_iter().collect();
        for c in &citer_set {
            self.references
                .entry(c.clone())
                .or_default()
                .insert(paper_id.to_string());
        }
        self.cited_by.insert(paper_id.to_string(), citer_set);
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        let mut all_nodes = HashSet::new();
        for k in self.references.keys() {
            all_nodes.insert(k.clone());
        }
        for k in self.cited_by.keys() {
            all_nodes.insert(k.clone());
        }
        for refs in self.references.values() {
            for r in refs {
                all_nodes.insert(r.clone());
            }
        }
        for citers in self.cited_by.values() {
            for c in citers {
                all_nodes.insert(c.clone());
            }
        }
        all_nodes.len()
    }

    /// Compute PageRank scores using power iteration.
    pub fn pagerank(&self, damping: f64, iterations: usize) -> Vec<(String, f64)> {
        let all_nodes: Vec<String> = {
            let mut nodes = HashSet::new();
            for k in self.references.keys() {
                nodes.insert(k.clone());
            }
            for k in self.cited_by.keys() {
                nodes.insert(k.clone());
            }
            for refs in self.references.values() {
                for r in refs {
                    nodes.insert(r.clone());
                }
            }
            for citers in self.cited_by.values() {
                for c in citers {
                    nodes.insert(c.clone());
                }
            }
            nodes.into_iter().collect()
        };

        let n = all_nodes.len();
        if n == 0 {
            return Vec::new();
        }

        let node_idx: HashMap<&str, usize> = all_nodes
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let mut scores = vec![1.0 / n as f64; n];

        for _ in 0..iterations {
            let mut new_scores = vec![(1.0 - damping) / n as f64; n];

            for (node, refs) in &self.references {
                if let Some(&from_idx) = node_idx.get(node.as_str()) {
                    let out_degree = refs.len();
                    if out_degree > 0 {
                        let share = scores[from_idx] * damping / out_degree as f64;
                        for r in refs {
                            if let Some(&to_idx) = node_idx.get(r.as_str()) {
                                new_scores[to_idx] += share;
                            }
                        }
                    }
                }
            }

            scores = new_scores;
        }

        let mut results: Vec<(String, f64)> = all_nodes
            .into_iter()
            .enumerate()
            .map(|(i, node)| (node, scores[i]))
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Find papers with the most shared references (bibliographic coupling).
    pub fn bibliographic_coupling(
        &self,
        paper_id: &str,
        max_results: usize,
    ) -> Vec<(String, usize)> {
        let target_refs = match self.references.get(paper_id) {
            Some(refs) => refs,
            None => return Vec::new(),
        };

        let mut coupling_scores: HashMap<String, usize> = HashMap::new();

        for (other_paper, other_refs) in &self.references {
            if other_paper == paper_id {
                continue;
            }
            let shared = target_refs.intersection(other_refs).count();
            if shared > 0 {
                coupling_scores.insert(other_paper.clone(), shared);
            }
        }

        let mut results: Vec<(String, usize)> = coupling_scores.into_iter().collect();
        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.truncate(max_results);
        results
    }

    /// Find shortest path between two papers using BFS.
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        if from == to {
            return Some(vec![from.to_string()]);
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parents: HashMap<String, String> = HashMap::new();

        visited.insert(from.to_string());
        queue.push_back(from.to_string());

        while let Some(current) = queue.pop_front() {
            // Get all neighbors (both directions)
            let mut neighbors = HashSet::new();
            if let Some(refs) = self.references.get(&current) {
                for r in refs {
                    neighbors.insert(r.clone());
                }
            }
            if let Some(citers) = self.cited_by.get(&current) {
                for c in citers {
                    neighbors.insert(c.clone());
                }
            }

            for neighbor in neighbors {
                if visited.contains(&neighbor) {
                    continue;
                }
                visited.insert(neighbor.clone());
                parents.insert(neighbor.clone(), current.clone());

                if neighbor == to {
                    // Reconstruct path
                    let mut path = vec![to.to_string()];
                    let mut curr = to.to_string();
                    while let Some(parent) = parents.get(&curr) {
                        path.push(parent.clone());
                        curr = parent.clone();
                    }
                    path.reverse();
                    return Some(path);
                }

                queue.push_back(neighbor);
            }
        }

        None
    }
}

// ── Topological Sort ──────────────────────────────────────────

/// Topological sort of blueprint components using Kahn's algorithm.
/// Returns ordered component IDs, or error with cycle description.
pub fn topological_sort(components: &[(String, Vec<String>)]) -> Result<Vec<String>, String> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    // Initialize all nodes
    for (id, _) in components {
        in_degree.entry(id.as_str()).or_insert(0);
        adjacency.entry(id.as_str()).or_default();
    }

    // Build graph
    for (id, deps) in components {
        for dep in deps {
            adjacency.entry(dep.as_str()).or_default().push(id.as_str());
            *in_degree.entry(id.as_str()).or_insert(0) += 1;
        }
    }

    // BFS with queue of zero in-degree nodes
    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|&(_, deg)| *deg == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut result = Vec::new();

    while let Some(node) = queue.pop_front() {
        result.push(node.to_string());
        if let Some(neighbors) = adjacency.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    if result.len() != components.len() {
        let remaining: Vec<&str> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg > 0)
            .map(|(&id, _)| id)
            .collect();
        return Err(format!(
            "Cycle detected among components: {}",
            remaining.join(", ")
        ));
    }

    Ok(result)
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_cache_insert_and_get() {
        let cache = ApiResponseCache::new(3600, 100);
        cache.insert("test_key".to_string(), serde_json::json!({"value": 42}));
        let result = cache.get("test_key");
        assert!(result.is_some());
        assert_eq!(result.unwrap()["value"], 42);
    }

    #[test]
    fn test_api_response_cache_miss() {
        let cache = ApiResponseCache::new(3600, 100);
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_api_response_cache_max_eviction() {
        let cache = ApiResponseCache::new(3600, 3);
        cache.insert("a".to_string(), serde_json::json!(1));
        cache.insert("b".to_string(), serde_json::json!(2));
        cache.insert("c".to_string(), serde_json::json!(3));
        assert_eq!(cache.len(), 3);

        // Inserting a 4th should evict the oldest
        cache.insert("d".to_string(), serde_json::json!(4));
        assert_eq!(cache.len(), 3);
        assert!(cache.get("d").is_some());
    }

    #[test]
    fn test_external_metadata_default() {
        let meta = ExternalMetadata::default();
        assert!(meta.citation_count.is_none());
        assert!(meta.tldr.is_none());
        assert!(meta.concepts.is_empty());
        assert!(meta.references.is_empty());
    }

    #[test]
    fn test_external_metadata_serde_roundtrip() {
        let meta = ExternalMetadata {
            citation_count: Some(150),
            influential_citation_count: Some(20),
            tldr: Some("A great paper.".to_string()),
            semantic_scholar_id: Some("abc123".to_string()),
            openalex_id: Some("W1234".to_string()),
            concepts: vec!["deep learning".to_string()],
            references: vec!["ref1".to_string()],
            cited_by: vec!["citer1".to_string()],
        };
        let json = serde_json::to_string(&meta).unwrap();
        let restored: ExternalMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.citation_count, Some(150));
        assert_eq!(restored.tldr.as_deref(), Some("A great paper."));
        assert_eq!(restored.concepts.len(), 1);
    }

    #[test]
    fn test_citation_graph_add_paper() {
        let mut graph = CitationGraph::new();
        graph.add_paper(
            "paper1",
            "Test Paper",
            10,
            vec!["ref1".to_string(), "ref2".to_string()],
            vec!["citer1".to_string()],
        );
        assert!(graph.titles.contains_key("paper1"));
        assert_eq!(graph.references["paper1"].len(), 2);
        assert!(graph.cited_by["paper1"].contains("citer1"));
    }

    #[test]
    fn test_citation_graph_pagerank_simple() {
        let mut graph = CitationGraph::new();
        // A -> B -> C (linear citation chain)
        graph.add_paper("A", "Paper A", 0, vec!["B".to_string()], vec![]);
        graph.add_paper(
            "B",
            "Paper B",
            1,
            vec!["C".to_string()],
            vec!["A".to_string()],
        );
        graph.add_paper("C", "Paper C", 2, vec![], vec!["B".to_string()]);

        let ranks = graph.pagerank(0.85, 20);
        assert_eq!(ranks.len(), 3);
        // C should rank highest (most cited in chain)
        assert_eq!(ranks[0].0, "C");
    }

    #[test]
    fn test_citation_graph_shortest_path() {
        let mut graph = CitationGraph::new();
        graph.add_paper("A", "Paper A", 0, vec!["B".to_string()], vec![]);
        graph.add_paper("B", "Paper B", 0, vec!["C".to_string()], vec![]);
        graph.add_paper("C", "Paper C", 0, vec![], vec![]);

        let path = graph.shortest_path("A", "C");
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path, vec!["A", "B", "C"]);

        // No path in disconnected graph
        graph.add_paper("D", "Paper D", 0, vec![], vec![]);
        let no_path = graph.shortest_path("D", "C");
        // D has no connections to A/B/C chain
        // Actually D was just added with no refs/citers, so it's isolated unless other
        // papers reference it. Let's check:
        assert!(no_path.is_none());
    }

    #[test]
    fn test_citation_graph_shortest_path_same_node() {
        let graph = CitationGraph::new();
        let path = graph.shortest_path("A", "A");
        assert_eq!(path, Some(vec!["A".to_string()]));
    }

    #[test]
    fn test_citation_graph_bibliographic_coupling() {
        let mut graph = CitationGraph::new();
        // Papers A and B both reference C and D
        graph.add_paper(
            "A",
            "Paper A",
            0,
            vec!["C".to_string(), "D".to_string()],
            vec![],
        );
        graph.add_paper(
            "B",
            "Paper B",
            0,
            vec!["C".to_string(), "D".to_string(), "E".to_string()],
            vec![],
        );

        let coupling = graph.bibliographic_coupling("A", 10);
        assert_eq!(coupling.len(), 1);
        assert_eq!(coupling[0].0, "B");
        assert_eq!(coupling[0].1, 2); // shared refs: C and D
    }

    #[test]
    fn test_citation_graph_node_count() {
        let mut graph = CitationGraph::new();
        graph.add_paper("A", "Paper A", 0, vec!["B".to_string()], vec![]);
        assert!(graph.node_count() >= 2);
    }

    #[test]
    fn test_citation_graph_state_roundtrip() {
        let mut graph = CitationGraph::new();
        graph.add_paper("A", "Paper A", 5, vec!["B".to_string()], vec![]);
        graph.add_paper("B", "Paper B", 10, vec![], vec!["A".to_string()]);

        let state = graph.to_state("A");
        let restored = CitationGraph::from_state(&state);
        assert_eq!(restored.titles.len(), graph.titles.len());
        assert_eq!(restored.citation_counts["A"], 5);
    }

    #[test]
    fn test_topological_sort_basic() {
        let components = vec![
            ("c".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
            ("a".to_string(), vec![]),
        ];
        let result = topological_sort(&components).unwrap();
        assert_eq!(result.len(), 3);
        // a must come before b, b before c
        let pos_a = result.iter().position(|x| x == "a").unwrap();
        let pos_b = result.iter().position(|x| x == "b").unwrap();
        let pos_c = result.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_topological_sort_diamond() {
        let components = vec![
            ("d".to_string(), vec!["b".to_string(), "c".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
            ("c".to_string(), vec!["a".to_string()]),
            ("a".to_string(), vec![]),
        ];
        let result = topological_sort(&components).unwrap();
        assert_eq!(result.len(), 4);
        let pos_a = result.iter().position(|x| x == "a").unwrap();
        let pos_b = result.iter().position(|x| x == "b").unwrap();
        let pos_c = result.iter().position(|x| x == "c").unwrap();
        let pos_d = result.iter().position(|x| x == "d").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn test_topological_sort_cycle_detection() {
        let components = vec![
            ("a".to_string(), vec!["b".to_string()]),
            ("b".to_string(), vec!["a".to_string()]),
        ];
        let result = topological_sort(&components);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cycle"));
    }
}

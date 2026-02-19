//! Package registry API client stub.
//!
//! Provides typed interfaces for enriching dependency graph nodes with metadata
//! from package registries (crates.io, npm, PyPI, etc.). This module defines
//! the data types and URL construction logic. Actual HTTP fetching is deferred
//! to a future implementation since it requires async HTTP and rate limiting.

use std::collections::HashMap;

/// Metadata retrieved from a package registry.
#[derive(Debug, Clone)]
pub struct RegistryMetadata {
    /// Package name.
    pub name: String,
    /// Latest published version.
    pub latest_version: String,
    /// License identifier (SPDX).
    pub license: Option<String>,
    /// Short description.
    pub description: Option<String>,
    /// Source repository URL.
    pub repository: Option<String>,
    /// Whether the package is deprecated.
    pub deprecated: bool,
}

/// Supported package registries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Registry {
    /// Rust (crates.io)
    CratesIo,
    /// JavaScript/Node (npmjs.com)
    Npm,
    /// Python (pypi.org)
    PyPi,
    /// Java/Kotlin (search.maven.org)
    MavenCentral,
    /// Ruby (rubygems.org)
    RubyGems,
    /// PHP (packagist.org)
    Packagist,
    /// Dart/Flutter (pub.dev)
    PubDev,
    /// Elixir/Erlang (hex.pm)
    HexPm,
    /// Swift (swiftpackageindex.com)
    SwiftPackageIndex,
    /// iOS/macOS (cocoapods.org)
    CocoaPods,
}

/// Client for looking up package metadata from registries.
///
/// Maintains an in-memory cache to avoid redundant lookups. The actual HTTP
/// fetching is not yet implemented — `lookup()` returns cached entries only,
/// and `insert()` allows pre-populating from external sources.
pub struct RegistryClient {
    cache: HashMap<(Registry, String), RegistryMetadata>,
}

impl RegistryClient {
    /// Create a new, empty registry client.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Look up cached metadata for a package in a given registry.
    ///
    /// Returns `None` if the package has not been fetched/cached yet.
    pub fn lookup(&self, registry: Registry, package: &str) -> Option<&RegistryMetadata> {
        self.cache.get(&(registry, package.to_string()))
    }

    /// Insert metadata into the cache (e.g., after an external fetch).
    pub fn insert(&mut self, registry: Registry, metadata: RegistryMetadata) {
        self.cache
            .insert((registry, metadata.name.clone()), metadata);
    }

    /// Check if metadata is cached for a package.
    pub fn is_cached(&self, registry: Registry, package: &str) -> bool {
        self.cache.contains_key(&(registry, package.to_string()))
    }

    /// Number of cached entries.
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Clear the entire cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the API URL for looking up a package on a given registry.
///
/// These URLs point to the JSON API endpoints used by each registry.
pub fn registry_url(registry: Registry, package: &str) -> String {
    match registry {
        Registry::CratesIo => {
            format!("https://crates.io/api/v1/crates/{package}")
        }
        Registry::Npm => {
            // npm supports scoped packages (@scope/name) — encode the slash
            let encoded = package.replace('/', "%2f");
            format!("https://registry.npmjs.org/{encoded}")
        }
        Registry::PyPi => {
            format!("https://pypi.org/pypi/{package}/json")
        }
        Registry::MavenCentral => {
            // Maven Central search API — expects group:artifact format
            format!("https://search.maven.org/solrsearch/select?q=a:{package}&rows=1&wt=json")
        }
        Registry::RubyGems => {
            format!("https://rubygems.org/api/v1/gems/{package}.json")
        }
        Registry::Packagist => {
            // Packagist expects vendor/package format
            format!("https://repo.packagist.org/p2/{package}.json")
        }
        Registry::PubDev => {
            format!("https://pub.dev/api/packages/{package}")
        }
        Registry::HexPm => {
            format!("https://hex.pm/api/packages/{package}")
        }
        Registry::SwiftPackageIndex => {
            // Swift Package Index API
            format!("https://swiftpackageindex.com/api/packages/{package}")
        }
        Registry::CocoaPods => {
            format!("https://trunk.cocoapods.org/api/v1/pods/{package}")
        }
    }
}

/// Map an ecosystem string (as used in `DepNode.ecosystem`) to a `Registry`.
///
/// Returns `None` for unrecognized ecosystems.
pub fn ecosystem_to_registry(ecosystem: &str) -> Option<Registry> {
    match ecosystem {
        "cargo" => Some(Registry::CratesIo),
        "npm" => Some(Registry::Npm),
        "pypi" => Some(Registry::PyPi),
        "maven" => Some(Registry::MavenCentral),
        "rubygems" => Some(Registry::RubyGems),
        "packagist" | "composer" => Some(Registry::Packagist),
        "pub" => Some(Registry::PubDev),
        "hex" => Some(Registry::HexPm),
        "swift" => Some(Registry::SwiftPackageIndex),
        "cocoapods" => Some(Registry::CocoaPods),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_url_crates_io() {
        assert_eq!(
            registry_url(Registry::CratesIo, "serde"),
            "https://crates.io/api/v1/crates/serde"
        );
    }

    #[test]
    fn test_registry_url_npm() {
        assert_eq!(
            registry_url(Registry::Npm, "lodash"),
            "https://registry.npmjs.org/lodash"
        );
    }

    #[test]
    fn test_registry_url_npm_scoped() {
        assert_eq!(
            registry_url(Registry::Npm, "@babel/core"),
            "https://registry.npmjs.org/@babel%2fcore"
        );
    }

    #[test]
    fn test_registry_url_pypi() {
        assert_eq!(
            registry_url(Registry::PyPi, "requests"),
            "https://pypi.org/pypi/requests/json"
        );
    }

    #[test]
    fn test_registry_url_rubygems() {
        assert_eq!(
            registry_url(Registry::RubyGems, "rails"),
            "https://rubygems.org/api/v1/gems/rails.json"
        );
    }

    #[test]
    fn test_registry_url_pub_dev() {
        assert_eq!(
            registry_url(Registry::PubDev, "flutter"),
            "https://pub.dev/api/packages/flutter"
        );
    }

    #[test]
    fn test_registry_url_hex_pm() {
        assert_eq!(
            registry_url(Registry::HexPm, "jason"),
            "https://hex.pm/api/packages/jason"
        );
    }

    #[test]
    fn test_registry_url_swift_package_index() {
        assert_eq!(
            registry_url(Registry::SwiftPackageIndex, "vapor/vapor"),
            "https://swiftpackageindex.com/api/packages/vapor/vapor"
        );
    }

    #[test]
    fn test_registry_url_cocoapods() {
        assert_eq!(
            registry_url(Registry::CocoaPods, "Alamofire"),
            "https://trunk.cocoapods.org/api/v1/pods/Alamofire"
        );
    }

    #[test]
    fn test_registry_url_maven() {
        assert_eq!(
            registry_url(Registry::MavenCentral, "gson"),
            "https://search.maven.org/solrsearch/select?q=a:gson&rows=1&wt=json"
        );
    }

    #[test]
    fn test_registry_url_packagist() {
        assert_eq!(
            registry_url(Registry::Packagist, "laravel/framework"),
            "https://repo.packagist.org/p2/laravel/framework.json"
        );
    }

    #[test]
    fn test_registry_client_lookup_empty() {
        let client = RegistryClient::new();
        assert!(client.lookup(Registry::CratesIo, "serde").is_none());
        assert_eq!(client.cache_size(), 0);
    }

    #[test]
    fn test_registry_client_insert_and_lookup() {
        let mut client = RegistryClient::new();

        client.insert(
            Registry::CratesIo,
            RegistryMetadata {
                name: "serde".into(),
                latest_version: "1.0.200".into(),
                license: Some("MIT OR Apache-2.0".into()),
                description: Some("A serialization framework".into()),
                repository: Some("https://github.com/serde-rs/serde".into()),
                deprecated: false,
            },
        );

        assert_eq!(client.cache_size(), 1);
        assert!(client.is_cached(Registry::CratesIo, "serde"));
        assert!(!client.is_cached(Registry::Npm, "serde"));

        let meta = client.lookup(Registry::CratesIo, "serde").unwrap();
        assert_eq!(meta.latest_version, "1.0.200");
        assert_eq!(meta.license.as_deref(), Some("MIT OR Apache-2.0"));
        assert!(!meta.deprecated);
    }

    #[test]
    fn test_registry_client_clear_cache() {
        let mut client = RegistryClient::new();
        client.insert(
            Registry::Npm,
            RegistryMetadata {
                name: "lodash".into(),
                latest_version: "4.17.21".into(),
                license: Some("MIT".into()),
                description: None,
                repository: None,
                deprecated: false,
            },
        );

        assert_eq!(client.cache_size(), 1);
        client.clear_cache();
        assert_eq!(client.cache_size(), 0);
    }

    #[test]
    fn test_ecosystem_to_registry() {
        assert_eq!(ecosystem_to_registry("cargo"), Some(Registry::CratesIo));
        assert_eq!(ecosystem_to_registry("npm"), Some(Registry::Npm));
        assert_eq!(ecosystem_to_registry("pypi"), Some(Registry::PyPi));
        assert_eq!(ecosystem_to_registry("pub"), Some(Registry::PubDev));
        assert_eq!(ecosystem_to_registry("hex"), Some(Registry::HexPm));
        assert_eq!(
            ecosystem_to_registry("swift"),
            Some(Registry::SwiftPackageIndex)
        );
        assert_eq!(ecosystem_to_registry("unknown"), None);
    }
}

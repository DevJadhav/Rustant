//! License compliance engine — SPDX identifier parsing and compatibility checking.
//!
//! Classifies licenses, checks compatibility, and evaluates against policies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// License classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LicenseClass {
    /// MIT, Apache-2.0, BSD-2/3, ISC, Unlicense, 0BSD.
    Permissive,
    /// LGPL, MPL, EPL.
    WeakCopyleft,
    /// GPL, AGPL.
    StrongCopyleft,
    /// Proprietary or commercial license.
    Proprietary,
    /// Unknown or custom license.
    Unknown,
}

impl std::fmt::Display for LicenseClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LicenseClass::Permissive => write!(f, "permissive"),
            LicenseClass::WeakCopyleft => write!(f, "weak copyleft"),
            LicenseClass::StrongCopyleft => write!(f, "strong copyleft"),
            LicenseClass::Proprietary => write!(f, "proprietary"),
            LicenseClass::Unknown => write!(f, "unknown"),
        }
    }
}

/// A detected license for a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageLicense {
    /// Package name.
    pub package: String,
    /// Package version.
    pub version: String,
    /// SPDX license identifier.
    pub spdx_id: String,
    /// Classified license type.
    pub class: LicenseClass,
    /// Whether this package offers dual licensing.
    pub dual_licensed: bool,
    /// Alternative SPDX identifiers (for dual-licensed packages).
    pub alternatives: Vec<String>,
}

/// Result of a license compliance check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseReport {
    /// All packages with their licenses.
    pub packages: Vec<PackageLicense>,
    /// Policy violations found.
    pub violations: Vec<LicenseViolation>,
    /// Packages needing manual review.
    pub review_required: Vec<PackageLicense>,
    /// Summary statistics.
    pub summary: LicenseSummary,
}

/// A license policy violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseViolation {
    /// Package that violates the policy.
    pub package: String,
    /// Version.
    pub version: String,
    /// The license that violates policy.
    pub license: String,
    /// Policy rule that was violated.
    pub rule: String,
    /// Severity of the violation.
    pub severity: ViolationSeverity,
}

/// Severity of a license violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViolationSeverity {
    /// Must be resolved before release.
    Blocker,
    /// Should be resolved.
    Warning,
    /// Informational.
    Info,
}

/// License distribution summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LicenseSummary {
    pub total_packages: usize,
    pub permissive: usize,
    pub weak_copyleft: usize,
    pub strong_copyleft: usize,
    pub proprietary: usize,
    pub unknown: usize,
}

/// License policy configuration.
#[derive(Debug, Clone, Default)]
pub struct LicensePolicy {
    /// Allowed SPDX identifiers (supports glob with "*").
    pub allowed: Vec<String>,
    /// Denied SPDX identifiers.
    pub denied: Vec<String>,
    /// Identifiers requiring manual review.
    pub review_required: Vec<String>,
    /// Package-specific overrides (package name → "approved" or "denied").
    pub overrides: HashMap<String, String>,
}

/// License compliance scanner.
pub struct LicenseScanner {
    policy: LicensePolicy,
}

impl LicenseScanner {
    /// Create with the given policy.
    pub fn new(policy: LicensePolicy) -> Self {
        Self { policy }
    }

    /// Create with a default permissive-only policy.
    pub fn permissive_only() -> Self {
        Self::new(LicensePolicy {
            allowed: vec![
                "MIT".into(),
                "Apache-2.0".into(),
                "BSD-2-Clause".into(),
                "BSD-3-Clause".into(),
                "ISC".into(),
                "Unlicense".into(),
                "0BSD".into(),
                "CC0-1.0".into(),
            ],
            denied: vec!["GPL-*".into(), "AGPL-*".into()],
            review_required: vec!["LGPL-*".into(), "MPL-*".into(), "EPL-*".into()],
            overrides: HashMap::new(),
        })
    }

    /// Classify an SPDX identifier.
    pub fn classify(spdx_id: &str) -> LicenseClass {
        let upper = spdx_id.to_uppercase();
        match upper.as_str() {
            "MIT" | "APACHE-2.0" | "BSD-2-CLAUSE" | "BSD-3-CLAUSE" | "ISC" | "UNLICENSE"
            | "0BSD" | "CC0-1.0" | "ZLIB" | "BSL-1.0" | "WTFPL" => LicenseClass::Permissive,
            s if s.starts_with("LGPL") || s.starts_with("MPL") || s.starts_with("EPL") => {
                LicenseClass::WeakCopyleft
            }
            s if s.starts_with("GPL") || s.starts_with("AGPL") => LicenseClass::StrongCopyleft,
            _ => LicenseClass::Unknown,
        }
    }

    /// Check a single package against the policy.
    pub fn check_package(&self, package: &str, spdx_id: &str) -> PolicyResult {
        // Check override first
        if let Some(override_val) = self.policy.overrides.get(package) {
            return if override_val == "approved" {
                PolicyResult::Allowed
            } else {
                PolicyResult::Denied("Package denied by override".into())
            };
        }

        // Check denied list
        if pattern_matches(&self.policy.denied, spdx_id) {
            return PolicyResult::Denied(format!("License '{spdx_id}' is in the denied list"));
        }

        // Check review required
        if pattern_matches(&self.policy.review_required, spdx_id) {
            return PolicyResult::ReviewRequired;
        }

        // Check allowed list (if not empty, act as allowlist)
        if !self.policy.allowed.is_empty() && !pattern_matches(&self.policy.allowed, spdx_id) {
            return PolicyResult::Denied(format!("License '{spdx_id}' is not in the allowed list"));
        }

        PolicyResult::Allowed
    }

    /// Generate a license report for a set of packages.
    pub fn check_packages(&self, packages: &[(String, String, String)]) -> LicenseReport {
        let mut pkg_licenses = Vec::new();
        let mut violations = Vec::new();
        let mut review_required = Vec::new();
        let mut summary = LicenseSummary::default();

        for (name, version, spdx_id) in packages {
            // Handle dual-licensing (SPDX "OR" expression)
            let license_ids: Vec<&str> = spdx_id.split(" OR ").map(|s| s.trim()).collect();

            let dual_licensed = license_ids.len() > 1;
            let primary = license_ids[0];
            let class = Self::classify(primary);

            let pkg = PackageLicense {
                package: name.clone(),
                version: version.clone(),
                spdx_id: primary.to_string(),
                class,
                dual_licensed,
                alternatives: license_ids[1..].iter().map(|s| s.to_string()).collect(),
            };

            // Update summary
            summary.total_packages += 1;
            match class {
                LicenseClass::Permissive => summary.permissive += 1,
                LicenseClass::WeakCopyleft => summary.weak_copyleft += 1,
                LicenseClass::StrongCopyleft => summary.strong_copyleft += 1,
                LicenseClass::Proprietary => summary.proprietary += 1,
                LicenseClass::Unknown => summary.unknown += 1,
            }

            // Check policy (try all license options for dual-licensed)
            let mut best_result = self.check_package(name, primary);
            if dual_licensed && !matches!(best_result, PolicyResult::Allowed) {
                for alt in &license_ids[1..] {
                    let alt_result = self.check_package(name, alt);
                    if matches!(alt_result, PolicyResult::Allowed) {
                        best_result = alt_result;
                        break;
                    }
                }
            }

            match best_result {
                PolicyResult::Denied(reason) => {
                    violations.push(LicenseViolation {
                        package: name.clone(),
                        version: version.clone(),
                        license: primary.to_string(),
                        rule: reason,
                        severity: ViolationSeverity::Blocker,
                    });
                }
                PolicyResult::ReviewRequired => {
                    review_required.push(pkg.clone());
                }
                PolicyResult::Allowed => {}
            }

            pkg_licenses.push(pkg);
        }

        LicenseReport {
            packages: pkg_licenses,
            violations,
            review_required,
            summary,
        }
    }
}

/// Result of a policy check on a single package.
#[derive(Debug, Clone)]
pub enum PolicyResult {
    Allowed,
    Denied(String),
    ReviewRequired,
}

/// Check if an SPDX identifier matches any pattern in a list.
/// Supports simple glob: "GPL-*" matches "GPL-2.0", "GPL-3.0".
fn pattern_matches(patterns: &[String], spdx_id: &str) -> bool {
    patterns.iter().any(|pattern| {
        if let Some(prefix) = pattern.strip_suffix('*') {
            spdx_id.starts_with(prefix)
        } else {
            pattern == spdx_id
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_license() {
        assert_eq!(LicenseScanner::classify("MIT"), LicenseClass::Permissive);
        assert_eq!(
            LicenseScanner::classify("Apache-2.0"),
            LicenseClass::Permissive
        );
        assert_eq!(
            LicenseScanner::classify("GPL-3.0"),
            LicenseClass::StrongCopyleft
        );
        assert_eq!(
            LicenseScanner::classify("LGPL-2.1"),
            LicenseClass::WeakCopyleft
        );
        assert_eq!(
            LicenseScanner::classify("some-custom-license"),
            LicenseClass::Unknown
        );
    }

    #[test]
    fn test_permissive_only_policy() {
        let scanner = LicenseScanner::permissive_only();

        assert!(matches!(
            scanner.check_package("foo", "MIT"),
            PolicyResult::Allowed
        ));
        assert!(matches!(
            scanner.check_package("bar", "GPL-3.0"),
            PolicyResult::Denied(_)
        ));
        assert!(matches!(
            scanner.check_package("baz", "LGPL-2.1"),
            PolicyResult::ReviewRequired
        ));
    }

    #[test]
    fn test_package_override() {
        let mut policy = LicensePolicy::default();
        policy.denied.push("GPL-*".into());
        policy
            .overrides
            .insert("special-pkg".into(), "approved".into());

        let scanner = LicenseScanner::new(policy);

        assert!(matches!(
            scanner.check_package("special-pkg", "GPL-3.0"),
            PolicyResult::Allowed
        ));
        assert!(matches!(
            scanner.check_package("other-pkg", "GPL-3.0"),
            PolicyResult::Denied(_)
        ));
    }

    #[test]
    fn test_check_packages() {
        let scanner = LicenseScanner::permissive_only();
        let packages = vec![
            ("serde".into(), "1.0.0".into(), "MIT OR Apache-2.0".into()),
            ("log".into(), "0.4.0".into(), "MIT".into()),
            ("gpl-lib".into(), "1.0.0".into(), "GPL-3.0".into()),
        ];

        let report = scanner.check_packages(&packages);

        assert_eq!(report.packages.len(), 3);
        assert_eq!(report.violations.len(), 1);
        assert_eq!(report.violations[0].package, "gpl-lib");
        assert_eq!(report.summary.total_packages, 3);
        assert_eq!(report.summary.permissive, 2);
    }

    #[test]
    fn test_dual_licensing() {
        let scanner = LicenseScanner::permissive_only();
        let packages = vec![("dual-pkg".into(), "1.0.0".into(), "GPL-3.0 OR MIT".into())];

        let report = scanner.check_packages(&packages);
        // Should be allowed because MIT is an alternative
        assert!(report.violations.is_empty());
        assert!(report.packages[0].dual_licensed);
    }

    #[test]
    fn test_pattern_matching() {
        assert!(pattern_matches(&["MIT".into()], "MIT"));
        assert!(pattern_matches(&["GPL-*".into()], "GPL-3.0"));
        assert!(!pattern_matches(&["GPL-*".into()], "LGPL-2.1"));
        assert!(!pattern_matches(&["MIT".into()], "Apache-2.0"));
    }

    #[test]
    fn test_license_summary() {
        let scanner = LicenseScanner::permissive_only();
        let packages = vec![
            ("a".into(), "1.0".into(), "MIT".into()),
            ("b".into(), "1.0".into(), "Apache-2.0".into()),
            ("c".into(), "1.0".into(), "GPL-3.0".into()),
        ];

        let report = scanner.check_packages(&packages);
        assert_eq!(report.summary.permissive, 2);
        assert_eq!(report.summary.strong_copyleft, 1);
    }
}

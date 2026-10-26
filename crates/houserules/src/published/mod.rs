//! The types that the published documents name. A published document is a
//! JSON document that `houserules` emits for another tool; it carries its
//! schema id and version in its first key, `schema`, and the committed
//! files under `schemas/` describe it.
//!
//! This module depends only on `serde`, `serde_json`, `schemars`, and
//! `indexmap`, so `src/bin/gen-schemas.rs` includes it with `#[path]` (the
//! package has no library target) and generates the schema of each document
//! from the type that serializes it.

use schemars::JsonSchema;
use serde::Serialize;

pub(crate) mod audit_result;
pub(crate) mod capture;
pub(crate) mod change;
pub(crate) mod change_record;
pub(crate) mod evidence;
pub(crate) mod intent;
pub(crate) mod route;
pub(crate) mod rule_package;

/// A document type that `houserules` publishes. The two constants build the
/// document's schema id and name its schema file.
pub(crate) trait Published: Serialize + JsonSchema {
    /// The document name in the schema id and in the schema path:
    /// `rule-package` in `houserules/rule-package@1.0.0`.
    const NAME: &'static str;
    /// The semver of the document's schema: `1.0.0` in
    /// `houserules/rule-package@1.0.0`.
    const VERSION: &'static str;
}

/// The schema id of the document type `T`: `houserules/<name>@<version>`.
/// The document holds it in its `schema` key.
pub(crate) fn id<T: Published>() -> String {
    format!("houserules/{}@{}", T::NAME, T::VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_joins_the_name_and_the_version() {
        assert_eq!(
            id::<rule_package::RulePackage>(),
            "houserules/rule-package@1.0.0"
        );
        assert_eq!(id::<capture::Header>(), "houserules/capture@1.0.0");
        assert_eq!(
            id::<audit_result::AuditResult>(),
            "houserules/audit-result@1.0.0"
        );
        assert_eq!(id::<route::RouteDocument>(), "houserules/route@1.0.0");
        assert_eq!(
            id::<change_record::ChangeRecord>(),
            "houserules/change-record@1.0.0"
        );
    }
}

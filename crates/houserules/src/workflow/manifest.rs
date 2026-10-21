//! The manifest of a workflow package and the files the package embeds.
//!
//! A package lives in `template/workflows/<name>/`: a `workflow.json`
//! manifest and the files it lists, each at its install path. The binary
//! embeds the whole tree. The manifest carries no `version`: the version of a
//! workflow is the version of the binary (`reference`).

use std::collections::BTreeMap;

use rust_embed::RustEmbed;
use serde::Deserialize;

/// The workflow packages of the kit, embedded from this repository's
/// `template/workflows/` at compile time.
#[derive(RustEmbed)]
#[folder = "../../template/workflows/"]
struct PackageFiles;

/// The name of the one workflow that the kit ships.
const REFERENCE: &str = "reference";

/// The manifest of a workflow package: `workflow.json` of its directory.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Manifest {
    /// The package name, which is also its directory below `workflows/`.
    pub(crate) name: String,
    /// The version of the binary. The manifest file holds no such key.
    #[serde(skip)]
    pub(crate) version: String,
    /// The schema id of the manifest, `houserules/workflow@<version>`.
    pub(crate) schema: String,
    /// The rule ids that the files of the package cite. `check-knowledge`
    /// checks that an installed, non-excluded entry provides each one.
    pub(crate) requires: Vec<String>,
    /// The install path of each file of the package, in the order `init`
    /// writes them.
    pub(crate) files: Vec<String>,
    /// The default model of each role. An install copies them to
    /// `workflow.models` of `.houserules.json`, which the project owns.
    pub(crate) models: BTreeMap<String, String>,
    /// The text that `render` writes into the AGENTS.md block while the
    /// workflow is installed.
    pub(crate) agents_block: String,
}

impl Manifest {
    /// The bytes of the package file at the install path `path`: a file that
    /// `files` lists, or a data file of the package. A path that the package
    /// does not hold is a build defect, so this function panics with the path.
    pub(crate) fn content(&self, path: &str) -> Vec<u8> {
        PackageFiles::get(&format!("{}/{path}", self.name))
            .unwrap_or_else(|| panic!("the embedded workflow {} lacks the file {path}", self.name))
            .data
            .into_owned()
    }
}

/// The reference workflow, as the binary carries it, at the version of the
/// binary.
///
/// The package is part of the binary, so a manifest that does not parse is a
/// build defect, not adopter data: this function panics with the parse error.
/// A test parses it.
pub(crate) fn reference() -> Manifest {
    let bytes = PackageFiles::get(&format!("{REFERENCE}/workflow.json"))
        .expect("the binary embeds the manifest of the reference workflow")
        .data;
    let mut manifest: Manifest = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("the reference workflow manifest is invalid: {error}"));
    manifest.version = env!("CARGO_PKG_VERSION").to_string();
    manifest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest parses, names the reference workflow, and takes its
    /// version from the binary.
    #[test]
    fn the_reference_manifest_parses_and_carries_the_version_of_the_binary() {
        let manifest = reference();
        assert_eq!(manifest.name, "reference");
        assert_eq!(manifest.schema, "houserules/workflow@1.0.0");
        assert_eq!(manifest.version, env!("CARGO_PKG_VERSION"));
        assert!(!manifest.requires.is_empty());
        assert!(
            manifest
                .agents_block
                .starts_with("This install runs the reference workflow.")
        );
    }

    /// Every file that the manifest lists is in the embedded package, and so
    /// is each data file of a workflow.
    #[test]
    fn every_listed_file_and_every_data_file_is_embedded() {
        let manifest = reference();
        let listed = manifest.files.iter().map(String::as_str);
        for path in listed.chain(crate::workflow::DATA_FILES.iter().copied()) {
            assert!(
                !manifest.content(path).is_empty(),
                "{path} is empty or missing"
            );
        }
    }

    /// The manifest holds no `version` key: a stray key fails the parse, so
    /// the version of the workflow has one source, the binary.
    #[test]
    fn a_manifest_with_a_version_key_is_refused() {
        let text = r#"{"name":"x","version":"1.0.0","schema":"s","requires":[],"files":[],"models":{},"agentsBlock":""}"#;
        let error = serde_json::from_str::<Manifest>(text).expect_err("a version key");
        assert!(
            error.to_string().contains("unknown field `version`"),
            "{error}"
        );
    }
}

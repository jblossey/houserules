//! The SARIF 2.1.0 log of a `runner` check with `parser: "sarif"`: the types
//! of the fields that `audit` reads, and the findings that count.
//!
//! The types hold the fields of the run, the invocation, and the result that
//! the finding rules need (spec 3.5) and ignore every other field. `parse`
//! rejects a document that is no JSON, has no `runs`, or says
//! `executionSuccessful: false`. `findings` assigns each result the level
//! that SARIF 2.1.0, section 3.27.10, assigns it and keeps the results of
//! level `error` and `warning`. A location's `uri` is resolved with the `url`
//! crate against its `uriBaseId` from `originalUriBaseIds`, or against the
//! install root, and a path below the root is made relative to it.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use percent_encoding::percent_decode_str;
use serde::Deserialize;
use url::{ParseError, Url};

use crate::published::audit_result::Finding;

/// What a SARIF document must not be: the reasons `parse` refuses it.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Fault {
    /// The tool printed nothing, or only white space.
    NoOutput,
    /// The text is no SARIF document; the first line of the parse error.
    Invalid(String),
    /// An invocation says `executionSuccessful: false`.
    ExecutionFailed,
}

/// A SARIF `level`. The four values are the whole SARIF enumeration; any
/// other text makes the document invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Level {
    None,
    Note,
    Warning,
    Error,
}

impl Level {
    /// `true` for the levels that count as a finding: `error` and `warning`.
    fn counts(self) -> bool {
        matches!(self, Level::Warning | Level::Error)
    }
}

/// One SARIF document, as `audit` reads it.
#[derive(Debug, Deserialize)]
pub(super) struct Log {
    runs: Vec<Run>,
}

/// One run of a tool.
#[derive(Debug, Deserialize)]
struct Run {
    #[serde(default)]
    tool: ToolInfo,
    #[serde(default)]
    invocations: Vec<Invocation>,
    #[serde(default, rename = "originalUriBaseIds")]
    original_uri_base_ids: HashMap<String, ArtifactLocation>,
    #[serde(default)]
    results: Vec<SarifResult>,
}

/// The `tool` of a run.
#[derive(Debug, Default, Deserialize)]
struct ToolInfo {
    #[serde(default)]
    driver: Driver,
}

/// The `tool.driver` of a run: the rules that it defines.
#[derive(Debug, Default, Deserialize)]
struct Driver {
    #[serde(default)]
    rules: Vec<Rule>,
}

/// A rule of `tool.driver.rules`.
#[derive(Debug, Deserialize)]
struct Rule {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "defaultConfiguration")]
    default_configuration: Option<Configuration>,
}

/// The configuration of a rule or of an override.
#[derive(Debug, Deserialize)]
struct Configuration {
    #[serde(default)]
    level: Option<Level>,
}

/// One invocation of a tool.
#[derive(Debug, Deserialize)]
struct Invocation {
    #[serde(default, rename = "executionSuccessful")]
    execution_successful: Option<bool>,
    #[serde(default, rename = "ruleConfigurationOverrides")]
    overrides: Vec<ConfigurationOverride>,
}

/// A `ruleConfigurationOverrides` entry of an invocation.
#[derive(Debug, Deserialize)]
struct ConfigurationOverride {
    #[serde(default)]
    descriptor: Descriptor,
    #[serde(default)]
    configuration: Option<Configuration>,
}

/// The `descriptor` of an override: the rule by position or by id.
#[derive(Debug, Default, Deserialize)]
struct Descriptor {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    index: Option<i64>,
}

/// One result of a run.
#[derive(Debug, Deserialize)]
struct SarifResult {
    #[serde(default, rename = "ruleId")]
    rule_id: Option<String>,
    #[serde(default, rename = "ruleIndex")]
    rule_index: Option<i64>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    level: Option<Level>,
    #[serde(default)]
    provenance: Option<Provenance>,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default)]
    locations: Vec<Location>,
}

/// The `provenance` of a result.
#[derive(Debug, Deserialize)]
struct Provenance {
    #[serde(default, rename = "invocationIndex")]
    invocation_index: Option<i64>,
}

/// A SARIF `message`.
#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    text: Option<String>,
}

/// A `location` of a result.
#[derive(Debug, Deserialize)]
struct Location {
    #[serde(default, rename = "physicalLocation")]
    physical_location: Option<PhysicalLocation>,
}

/// The `physicalLocation` of a location.
#[derive(Debug, Deserialize)]
struct PhysicalLocation {
    #[serde(default, rename = "artifactLocation")]
    artifact_location: Option<ArtifactLocation>,
    #[serde(default)]
    region: Option<Region>,
}

/// An `artifactLocation`: a URI, relative to the base that `uriBaseId` names.
#[derive(Debug, Deserialize)]
struct ArtifactLocation {
    #[serde(default)]
    uri: Option<String>,
    #[serde(default, rename = "uriBaseId")]
    uri_base_id: Option<String>,
}

/// The `region` of a physical location.
#[derive(Debug, Deserialize)]
struct Region {
    #[serde(default, rename = "startLine")]
    start_line: Option<u64>,
}

/// The rule of a result that names no rule.
const NO_RULE_ID: &str = "(no rule id)";

/// The message of a result that has no text.
const NO_MESSAGE: &str = "(no message)";

/// The deepest chain of `uriBaseId` references that `findings` follows.
const MAX_BASE_DEPTH: usize = 8;

/// Reads the SARIF document that a tool printed on stdout.
pub(super) fn parse(stdout: &[u8]) -> Result<Log, Fault> {
    if stdout.iter().all(u8::is_ascii_whitespace) {
        return Err(Fault::NoOutput);
    }
    let log: Log = serde_json::from_slice(stdout).map_err(|error| {
        let message = error.to_string();
        Fault::Invalid(message.lines().next().unwrap_or_default().to_string())
    })?;
    let failed = log
        .runs
        .iter()
        .flat_map(|run| &run.invocations)
        .any(|invocation| invocation.execution_successful == Some(false));
    if failed {
        return Err(Fault::ExecutionFailed);
    }
    Ok(log)
}

/// The results of `log` that count, in the order of the document. `root` is
/// the install root that a path is made relative to.
pub(super) fn findings(log: &Log, root: &Path) -> Vec<Finding> {
    findings_under(log, &root_urls(root))
}

/// The results of `log` that count, with every path made relative to the
/// first of `roots` that holds it. Each root is a directory URL.
fn findings_under(log: &Log, roots: &[Url]) -> Vec<Finding> {
    log.runs
        .iter()
        .flat_map(|run| {
            run.results
                .iter()
                .filter(|result| run.level_of(result).counts())
                .map(|result| run.finding(result, roots))
                .collect::<Vec<_>>()
        })
        .collect()
}

impl Run {
    /// The position of the rule that `result` names in `tool.driver.rules`:
    /// by `ruleIndex`, else by `ruleId`.
    fn rule_position(&self, result: &SarifResult) -> Option<usize> {
        let rules = &self.tool.driver.rules;
        let by_index = result
            .rule_index
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < rules.len());
        by_index.or_else(|| {
            let id = non_blank(result.rule_id.as_deref())?;
            rules.iter().position(|rule| rule.id.as_deref() == Some(id))
        })
    }

    /// The id of the rule of `result`: its `ruleId`, else the id of the rule
    /// at `position`. An empty or blank id is absent.
    fn rule_id<'a>(&'a self, result: &'a SarifResult, position: Option<usize>) -> Option<&'a str> {
        non_blank(result.rule_id.as_deref()).or_else(|| {
            let rule = self.tool.driver.rules.get(position?)?;
            non_blank(rule.id.as_deref())
        })
    }

    /// The level of `result` by SARIF 2.1.0, section 3.27.10: `none` for a
    /// `kind` other than `fail`; else `result.level`; else the level of the
    /// first override of the invocation that `provenance.invocationIndex`
    /// names and whose descriptor names the rule; else the default level of
    /// the rule; else `warning`.
    fn level_of(&self, result: &SarifResult) -> Level {
        if result.kind.as_deref().is_some_and(|kind| kind != "fail") {
            return Level::None;
        }
        if let Some(level) = result.level {
            return level;
        }
        let position = self.rule_position(result);
        self.override_level(result, position)
            .or_else(|| {
                let rule = self.tool.driver.rules.get(position?)?;
                rule.default_configuration.as_ref()?.level
            })
            .unwrap_or(Level::Warning)
    }

    /// The level that an override of the invocation of `result` sets for its
    /// rule, or `None` when no override does.
    fn override_level(&self, result: &SarifResult, position: Option<usize>) -> Option<Level> {
        let invocation = result
            .provenance
            .as_ref()?
            .invocation_index
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.invocations.get(index))?;
        let id = self.rule_id(result, position);
        invocation
            .overrides
            .iter()
            .find(|entry| {
                let descriptor = &entry.descriptor;
                match descriptor
                    .index
                    .and_then(|index| usize::try_from(index).ok())
                {
                    Some(index) => Some(index) == position,
                    None => descriptor.id.is_some() && descriptor.id.as_deref() == id,
                }
            })?
            .configuration
            .as_ref()?
            .level
    }

    /// The finding that `result` makes.
    fn finding(&self, result: &SarifResult, roots: &[Url]) -> Finding {
        let position = self.rule_position(result);
        let located = result.locations.iter().find_map(|location| {
            let physical = location.physical_location.as_ref()?;
            let artifact = physical.artifact_location.as_ref()?;
            let uri = artifact.uri.as_deref()?;
            let line = physical
                .region
                .as_ref()
                .and_then(|region| region.start_line);
            Some((
                self.path_of(uri, artifact.uri_base_id.as_deref(), roots),
                line,
            ))
        });
        let (path, line) = match located {
            Some((path, line)) => (Some(path), line),
            None => (None, None),
        };
        Finding {
            path,
            line,
            rule: self
                .rule_id(result, position)
                .unwrap_or(NO_RULE_ID)
                .to_string(),
            message: result
                .message
                .as_ref()
                .and_then(|message| non_blank(message.text.as_deref()))
                .unwrap_or(NO_MESSAGE)
                .to_string(),
        }
    }

    /// The path of the artifact at `uri`. The reference is resolved (RFC 3986)
    /// against the base that `uri_base_id` names in `originalUriBaseIds`, else
    /// against the install root. A `file:` path below one of `roots` is made
    /// relative to it and percent-decoded. A path outside every root keeps the
    /// fully resolved URI when a base of the run was applied, because the bare
    /// reference would read as a path below the root. In every other case `uri`
    /// stays as the tool wrote it: a URI of another scheme, a `file:` URI with a
    /// host other than `localhost`, a reference that leaves the root, and a path
    /// that does not decode as text.
    fn path_of(&self, uri: &str, uri_base_id: Option<&str>, roots: &[Url]) -> String {
        let (resolved, base_applied) = match Url::parse(uri) {
            Ok(absolute) => (absolute, false),
            Err(ParseError::RelativeUrlWithoutBase) => {
                let defined = uri_base_id.and_then(|id| self.base_url(id, roots, 0));
                let Some(base) = defined.as_ref().or_else(|| roots.first()) else {
                    return uri.to_string();
                };
                match base.join(uri) {
                    Ok(joined) => (joined, defined.is_some()),
                    Err(_) => return uri.to_string(),
                }
            }
            Err(_) => return uri.to_string(),
        };
        match path_below_root(&resolved, roots) {
            Some(encoded) => percent_decode_str(encoded)
                .decode_utf8()
                .map_or_else(|_| uri.to_string(), Cow::into_owned),
            None if base_applied => resolved.to_string(),
            None => uri.to_string(),
        }
    }

    /// The absolute URL of the base `id` of `originalUriBaseIds`: its `uri`
    /// when that is absolute, else the `uri` resolved against the base that
    /// the entry names itself, else against the install root. `None` for an
    /// unknown id, an entry with no usable `uri`, and a chain of more than
    /// [`MAX_BASE_DEPTH`] steps.
    fn base_url(&self, id: &str, roots: &[Url], depth: usize) -> Option<Url> {
        if depth >= MAX_BASE_DEPTH {
            return None;
        }
        let entry = self.original_uri_base_ids.get(id)?;
        let uri = entry.uri.as_deref()?;
        match Url::parse(uri) {
            Ok(absolute) => Some(absolute),
            Err(ParseError::RelativeUrlWithoutBase) => {
                let parent = entry
                    .uri_base_id
                    .as_deref()
                    .and_then(|parent| self.base_url(parent, roots, depth + 1));
                parent.as_ref().or_else(|| roots.first())?.join(uri).ok()
            }
            Err(_) => None,
        }
    }
}

/// `text` unless it is absent, empty, or only white space.
fn non_blank(text: Option<&str>) -> Option<&str> {
    text.filter(|text| !text.trim().is_empty())
}

/// The percent-encoded path of the `file:` URL `url` below the first of
/// `roots` that holds it, with no leading `/`. `None` for a URL of another
/// scheme or host, one outside every root, and a root itself.
fn path_below_root<'a>(url: &'a Url, roots: &[Url]) -> Option<&'a str> {
    if url.scheme() != "file" {
        return None;
    }
    roots
        .iter()
        .filter(|root| root.host_str() == url.host_str())
        .find_map(|root| url.path().strip_prefix(root.path()))
        .filter(|below| !below.is_empty())
}

/// The directory URLs of `root` that a tool may print: the root as it is made
/// absolute, and its canonical form when that differs.
fn root_urls(root: &Path) -> Vec<Url> {
    let mut urls = Vec::new();
    for path in [
        std::path::absolute(root).ok(),
        std::fs::canonicalize(root).ok(),
    ]
    .into_iter()
    .flatten()
    {
        if let Ok(url) = Url::from_directory_path(&path)
            && !urls.contains(&url)
        {
            urls.push(url);
        }
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The run of `document`, the first of its `runs`.
    fn first_run(document: &str) -> Log {
        parse(document.as_bytes()).expect("a valid document")
    }

    /// The directory URL `url`.
    fn directory(url: &str) -> Url {
        Url::parse(url).expect("a URL")
    }

    /// The findings of `document` under the root `/work/repo`.
    fn findings_under_root(document: &str) -> Vec<Finding> {
        findings_under(&first_run(document), &[directory("file:///work/repo/")])
    }

    /// The paths of the findings of a run whose results name `uris`.
    fn paths_of(base_ids: &str, uris: &[&str]) -> Vec<Option<String>> {
        let results: Vec<String> = uris
            .iter()
            .map(|uri| {
                format!(
                    r#"{{"ruleId":"r","level":"error","locations":[{{"physicalLocation":{{"artifactLocation":{{"uri":"{uri}","uriBaseId":"BASE"}}}}}}]}}"#
                )
            })
            .collect();
        let document = format!(
            r#"{{"runs":[{{"originalUriBaseIds":{base_ids},"results":[{}]}}]}}"#,
            results.join(",")
        );
        findings_under_root(&document)
            .into_iter()
            .map(|finding| finding.path)
            .collect()
    }

    /// A percent escape is decoded to text, and a `%` that no two hexadecimal
    /// digits follow stays as it is.
    #[test]
    fn escapes_are_decoded_and_a_malformed_escape_stays() {
        let paths = paths_of(
            "{}",
            &[
                "src/a%20b.rs",
                "src/100%.rs",
                "src/%zz%4.rs",
                "src/%C3%A9.rs",
            ],
        );
        assert_eq!(
            paths,
            [
                Some("src/a b.rs".to_string()),
                Some("src/100%.rs".to_string()),
                Some("src/%zz%4.rs".to_string()),
                Some("src/\u{e9}.rs".to_string()),
            ]
        );
    }

    /// The host `localhost` is the local machine; another host, and a scheme
    /// other than `file`, keep the URI as given.
    #[test]
    fn file_uris_name_the_local_machine_only() {
        let paths = paths_of(
            "{}",
            &[
                "file://localhost/work/repo/src/a.rs",
                "file://build-host/work/repo/src/a.rs",
                "https://example.org/work/repo/src/a.rs",
            ],
        );
        assert_eq!(
            paths,
            [
                Some("src/a.rs".to_string()),
                Some("file://build-host/work/repo/src/a.rs".to_string()),
                Some("https://example.org/work/repo/src/a.rs".to_string()),
            ]
        );
    }

    /// A query or a fragment is no part of the path of a file.
    #[test]
    fn a_query_or_fragment_is_no_part_of_the_path() {
        let paths = paths_of("{}", &["src/a.rs#L3", "src/b.rs?x=1"]);
        assert_eq!(
            paths,
            [Some("src/a.rs".to_string()), Some("src/b.rs".to_string())]
        );
    }

    /// A drive path in a `file:` URI is made relative to a Windows root,
    /// whatever the build host: the comparison is on the URL.
    #[test]
    fn a_drive_path_under_the_root_is_made_relative() {
        let document = r#"{"runs":[{"results":[{"ruleId":"r","level":"error","locations":[{"physicalLocation":{"artifactLocation":{"uri":"file:///C:/Users/x/repo/src/a%20b.rs"}}}]}]}]}"#;
        let found = findings_under(
            &first_run(document),
            &[directory("file:///C:/Users/x/repo/")],
        );
        assert_eq!(found[0].path.as_deref(), Some("src/a b.rs"));
    }

    /// The roots are directory URLs, the first of them the root as it is made
    /// absolute.
    #[test]
    fn roots_are_directory_urls() {
        let directory = std::env::temp_dir();
        let urls = root_urls(&directory);
        assert!(
            urls.iter()
                .all(|url| url.scheme() == "file" && url.path().ends_with('/')),
            "{urls:?}"
        );
        assert_eq!(
            urls.first(),
            Url::from_directory_path(std::path::absolute(&directory).expect("absolute"))
                .as_ref()
                .ok()
        );
    }

    /// The path of the root itself names no file: the URI stays as given.
    #[test]
    fn the_root_itself_is_kept_as_given() {
        let paths = paths_of("{}", &["file:///work/repo/"]);
        assert_eq!(paths, [Some("file:///work/repo/".to_string())]);
    }

    /// A relative reference that leaves the root, a URI with another scheme,
    /// and an escape that decodes to no text stay as the tool wrote them.
    #[test]
    fn paths_outside_the_root_or_not_files_are_kept_as_given() {
        let paths = paths_of(
            "{}",
            &[
                "../outside/a.rs",
                "https://example.org/a.rs",
                "src/%ff.rs",
                "file:///work/other/a.rs",
            ],
        );
        assert_eq!(
            paths,
            [
                Some("../outside/a.rs".to_string()),
                Some("https://example.org/a.rs".to_string()),
                Some("src/%ff.rs".to_string()),
                Some("file:///work/other/a.rs".to_string()),
            ]
        );
    }

    /// A base id chain resolves to the root, and a chain that loops ends at
    /// the depth limit with the URI as given.
    #[test]
    fn base_ids_resolve_through_a_chain_and_stop_in_a_loop() {
        let chain = r#"{"BASE":{"uri":"repo/","uriBaseId":"ROOT"},"ROOT":{"uri":"file:///work/"}}"#;
        assert_eq!(
            paths_of(chain, &["src/a.rs"]),
            [Some("src/a.rs".to_string())]
        );
        let looped = r#"{"BASE":{"uri":"x/","uriBaseId":"BASE"}}"#;
        assert_eq!(
            paths_of(looped, &["src/a.rs"]),
            [Some(format!("{}src/a.rs", "x/".repeat(MAX_BASE_DEPTH)))]
        );
    }

    /// A base outside the root gives the fully resolved URI of the location,
    /// whatever its scheme; a base below the root gives a path below the root,
    /// with the dot segments removed.
    #[test]
    fn a_base_outside_the_root_gives_the_resolved_uri() {
        let outside = r#"{"BASE":{"uri":"file:///elsewhere/"}}"#;
        assert_eq!(
            paths_of(outside, &["src/a.rs"]),
            [Some("file:///elsewhere/src/a.rs".to_string())]
        );
        let remote = r#"{"BASE":{"uri":"https://example.org/repo/"}}"#;
        assert_eq!(
            paths_of(remote, &["src/a.rs"]),
            [Some("https://example.org/repo/src/a.rs".to_string())]
        );
        let below = r#"{"BASE":{"uri":"file:///work/repo/src/"}}"#;
        assert_eq!(
            paths_of(below, &["../src/a.rs", "./b.rs"]),
            [Some("src/a.rs".to_string()), Some("src/b.rs".to_string())]
        );
    }

    /// An empty or blank `ruleId`, a blank id in `tool.driver.rules`, and an
    /// empty or blank message text are absent.
    #[test]
    fn blank_ids_and_messages_are_absent() {
        let document = r#"{"runs":[{
            "tool":{"driver":{"rules":[{"id":" "}]}},
            "results":[
                {"ruleId":"","ruleIndex":0,"level":"error","message":{"text":" "}},
                {"ruleId":"  ","level":"error","message":{"text":""}}
            ]}]}"#;
        let found = findings_under_root(document);
        assert_eq!(found.len(), 2);
        for finding in found {
            assert_eq!(finding.rule, NO_RULE_ID);
            assert_eq!(finding.message, NO_MESSAGE);
        }
    }

    /// An override entry with no level leaves the level to the rule default,
    /// and an invocation index past the end names no invocation.
    #[test]
    fn an_override_without_a_level_and_a_bad_invocation_index_fall_through() {
        let document = |index: i64| {
            format!(
                r#"{{"runs":[{{
                "tool":{{"driver":{{"rules":[{{"id":"r","defaultConfiguration":{{"level":"note"}}}}]}}}},
                "invocations":[{{"ruleConfigurationOverrides":[{{"descriptor":{{"id":"r"}},"configuration":{{}}}}]}}],
                "results":[{{"ruleId":"r","provenance":{{"invocationIndex":{index}}}}}]}}]}}"#
            )
        };
        assert_eq!(findings_under_root(&document(0)), []);
        assert_eq!(findings_under_root(&document(5)), []);
        assert_eq!(findings_under_root(&document(-1)), []);
    }

    /// A level outside the four of SARIF makes the document invalid.
    #[test]
    fn an_unknown_level_is_invalid_sarif() {
        let error =
            parse(br#"{"runs":[{"results":[{"level":"fatal"}]}]}"#).expect_err("an unknown level");
        assert!(
            matches!(error, Fault::Invalid(line) if line.starts_with("unknown variant `fatal`"))
        );
    }

    /// White space is no output.
    #[test]
    fn white_space_is_no_output() {
        assert_eq!(parse(b" \n\t").expect_err("no output"), Fault::NoOutput);
    }
}

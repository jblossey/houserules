//! The texts that `ci print` prints and `ci write` writes (spec 10.2 and 10.3): the reference form
//! of each host, which calls the published unit of this release, and the inline form, which is the
//! unit with every input bound for one install.
//!
//! A project-derived string (an install path, a key, a branch) is a JSON string in every form, with
//! every control character written as an escape (`review::text::json_string`), so that no control
//! character of it changes the YAML around it. The inline forms are made from the bytes of
//! the unit that the binary embeds. They change the `on` block of the GitHub unit and drop the
//! `spec` header of the GitLab unit, and they replace each `inputs.<name>` by its value. Nothing
//! else of the unit changes: the trust rules of spec 10.1 hold for the form because they hold for
//! the unit, except that a step of the GitHub unit whose `if` compares an input with a string is kept
//! without its `if` or dropped, as the comparison says, because a constant condition is a defect of
//! a workflow. A unit that reads an input this module does not bind stops the command with a panic,
//! which the tests over the embedded units reach before a release.
//!
//! The GitHub form installs `release` for the floor, so it keeps no checkout of the default branch.
//! It keeps `HOUSERULES_DEFAULT_SRC` in the install step of the floor, and the comment above the
//! checkout of the request, which names the default branch. Both are the text of the unit. The
//! shell functions that the form carries whole read the variable in the branch for
//! `floor-install: default-branch`, and the form binds `floor-install` to `release`, so that
//! branch never runs.

use saphyr::{LoadableYamlNode, Yaml};

use crate::review::text::json_string;

/// The path of the GitHub unit as a caller names it.
const GITHUB_UNIT_PATH: &str = "jblossey/houserules/.github/workflows/houserules-review.yml";

/// The comment that marks the release version line of the GitLab unit for the release tooling of
/// this repository. A form leaves it out: it means nothing in the pipeline of a project.
const RELEASE_ANNOTATION: &str = " # x-release-please-version";

/// The placeholder for the commit of the release that `ci print` leaves in a caller.
const SHA_PLACEHOLDER: &str = "<release commit sha>";

/// What a form is written for: one install of a repository.
pub(super) struct Wiring<'a> {
    /// The version of the binary, which the forms pin.
    pub(super) version: &'a str,
    /// The install path below the git top level, `.` for the top level.
    pub(super) path: &'a str,
    /// The key of the install.
    pub(super) key: &'a str,
    /// The tracker input of the units: `github` or `beads`, the trackers that need access in a job.
    pub(super) tracker: Option<&'a str>,
}

/// The command that resolves the commit of the tag of release `version`.
pub(super) fn ls_remote(version: &str) -> String {
    format!("git ls-remote https://github.com/jblossey/houserules refs/tags/v{version}")
}

/// The `on` block of a GitHub workflow that runs the review layer: requests (with the edit of a
/// description) and pushes to the default branch `branch`.
fn github_triggers(branch: &str) -> String {
    format!(
        "on:\n  pull_request:\n    types: [opened, synchronize, reopened, edited]\n  push:\n    branches: [{}]\n",
        json_string(branch)
    )
}

/// The caller of the GitHub unit (the reference form), pinned to the commit `sha` and triggered by a
/// push to the default branch `branch`. Without `sha` the caller holds a placeholder, and a comment
/// line says how to resolve it. The caller grants the union of the `permissions` of the five jobs of
/// the unit; each job narrows its own token (spec 10.2).
pub(super) fn github_reference(wiring: &Wiring<'_>, sha: Option<&str>, branch: &str) -> String {
    let version = wiring.version;
    let mut text = String::new();
    if sha.is_none() {
        text.push_str(&format!(
            "# Resolve {SHA_PLACEHOLDER} with: {}\n",
            ls_remote(version)
        ));
    }
    text.push_str("name: houserules review\n");
    text.push_str(&github_triggers(branch));
    text.push_str("jobs:\n  review:\n    permissions:\n");
    for scope in [
        "contents: write",
        "pull-requests: write",
        "statuses: write",
        "issues: write",
    ] {
        text.push_str(&format!("      {scope}\n"));
    }
    text.push_str(&format!(
        "    uses: {GITHUB_UNIT_PATH}@{} # v{version}\n",
        sha.unwrap_or(SHA_PLACEHOLDER)
    ));
    text.push_str(&format!("    with:\n      version: {version}\n"));
    text.push_str(&format!("      dir: {}\n", json_string(wiring.path)));
    text.push_str(&format!("      key: {}\n", json_string(wiring.key)));
    if let Some(tracker) = wiring.tracker {
        text.push_str(&format!("      tracker: {tracker}\n"));
    }
    text
}

/// The include of the GitLab unit of release `wiring.version` (the reference form), with the
/// `integrity` value of the file (GitLab 17.9) and the inputs of the install.
pub(super) fn gitlab_reference(wiring: &Wiring<'_>, integrity: &str) -> String {
    let mut text = format!(
        "include:\n  - remote: https://raw.githubusercontent.com/jblossey/houserules/v{}/ci/gitlab-review.yml\n    integrity: {integrity}\n    inputs:\n",
        wiring.version
    );
    text.push_str(&format!("      dir: {}\n", json_string(wiring.path)));
    text.push_str(&format!("      key: {}\n", json_string(wiring.key)));
    if let Some(tracker) = wiring.tracker {
        text.push_str(&format!("      tracker: {tracker}\n"));
    }
    text
}

/// The line that includes the file at `path`, for the `.gitlab-ci.yml` of a project.
pub(super) fn include_line(path: &str) -> String {
    format!("include: [{{ local: {} }}]", json_string(path))
}

/// `true` when an expression that sits between `before` and `after` is the whole value of a YAML
/// key: `before` ends with `: ` on its line and `after` starts a new line or ends the text.
fn is_whole_value(before: &str, after: &str) -> bool {
    let line = before.rsplit('\n').next().unwrap_or_default();
    line.ends_with(": ") && (after.is_empty() || after.starts_with('\n'))
}

/// The name at the start of `text`: it runs over letters, digits, `_`, and `-`.
fn name_after(text: &str) -> &str {
    let end = text
        .find(|character: char| {
            !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
        })
        .unwrap_or(text.len());
    &text[..end]
}

/// Replaces each `<prefix><name>` of `expression` by the string literal of `value(name)`.
fn bind_in_expression(expression: &str, prefix: &str, value: &dyn Fn(&str) -> String) -> String {
    let mut bound = String::new();
    let mut rest = expression;
    while let Some(at) = rest.find(prefix) {
        bound.push_str(&rest[..at]);
        let after = &rest[at + prefix.len()..];
        let name = name_after(after);
        bound.push_str(&format!("'{}'", value(name).replace('\'', "''")));
        rest = &after[name.len()..];
    }
    bound.push_str(rest);
    bound
}

/// Binds every input in `text`. `open` and `close` delimit an expression (`${{`/`}}` on GitHub,
/// `$[[`/`]]` on GitLab), `prefix` starts an input name inside one, and `value` gives the value of an
/// input. An expression that is exactly one input is replaced by the JSON string of its value when it
/// is the whole value of a key, and by the raw value inside a name (only the key of the install is
/// read there); an expression that holds an input among other terms gets a string literal for each
/// input.
fn bind(
    text: &str,
    (open, close): (&str, &str),
    prefix: &str,
    value: &dyn Fn(&str) -> String,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let from = &rest[start..];
        let end = from
            .find(close)
            .map(|at| at + close.len())
            .unwrap_or_else(|| panic!("an expression of the unit does not close"));
        let expression = &from[..end];
        let after = &from[end..];
        let inner = expression[open.len()..end - close.len()].trim();
        if let Some(name) = inner
            .strip_prefix(prefix)
            .filter(|name| name_after(name) == *name)
        {
            if is_whole_value(&out, after) {
                out.push_str(&json_string(&value(name)));
            } else {
                assert_eq!(
                    name, "key",
                    "only the key of the install sits inside a name"
                );
                out.push_str(&value(name));
            }
        } else if let Some((name, equal, literal)) =
            comparison(inner, prefix).filter(|_| is_whole_value(&out, after))
        {
            out.push_str(if (value(name) == literal) == equal {
                "true"
            } else {
                "false"
            });
        } else if inner.contains(prefix) {
            out.push_str(open);
            out.push_str(&bind_in_expression(
                &expression[open.len()..end - close.len()],
                prefix,
                value,
            ));
            out.push_str(close);
        } else {
            out.push_str(expression);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The comparison that `inner` is, when it is exactly `<prefix><name> == '<literal>'` or
/// `<prefix><name> != '<literal>'`: the name, whether the operator is `==`, and the literal.
fn comparison<'a>(inner: &'a str, prefix: &str) -> Option<(&'a str, bool, &'a str)> {
    let rest = inner.strip_prefix(prefix)?;
    let name = name_after(rest);
    let rest = rest[name.len()..].trim_start();
    let (equal, rest) = match rest.strip_prefix("==") {
        Some(rest) => (true, rest),
        None => (false, rest.strip_prefix("!=")?),
    };
    let literal = rest.trim().strip_prefix('\'')?.strip_suffix('\'')?;
    (!literal.contains('\'')).then_some((name, equal, literal))
}

/// The indent of a step in a list of steps.
const STEP_INDENT: &str = "      - ";

/// The indent of the keys of a step.
const STEP_KEY_INDENT: &str = "        ";

/// What the `if` of the step `step` says when it compares an input: `Some(holds)`. A step with no
/// `if`, and one whose `if` reads no input, give `None`. An `if` that reads an input in another
/// form than a comparison is a defect of the unit.
fn input_condition(step: &[&str], value: &dyn Fn(&str) -> String) -> Option<bool> {
    let line = step
        .iter()
        .find(|line| line.starts_with(&format!("{STEP_KEY_INDENT}if: ${{{{")))?;
    let inner = line
        .trim_start()
        .strip_prefix("if: ${{")?
        .strip_suffix("}}")?
        .trim();
    if !inner.contains("inputs.") {
        return None;
    }
    let (name, equal, literal) = comparison(inner, "inputs.").unwrap_or_else(|| {
        panic!("the unit reads an input in a condition that ci cannot settle: {inner}")
    });
    Some((value(name) == literal) == equal)
}

/// `text`, a workflow, without each step whose `if` compares an input and fails, and without the
/// `if` of each step whose comparison holds.
fn settle_steps(text: &str, value: &dyn Fn(&str) -> String) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut in_steps = false;
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if line == "    steps:" {
            in_steps = true;
        } else if !line.trim().is_empty() && !line.starts_with("      ") {
            in_steps = false;
        }
        if !(in_steps && line.starts_with(STEP_INDENT)) {
            out.push(line);
            at += 1;
            continue;
        }
        let mut end = at + 1;
        while end < lines.len()
            && (lines[end].trim().is_empty() || lines[end].starts_with(STEP_KEY_INDENT))
        {
            end += 1;
        }
        let mut body_end = end;
        while body_end > at + 1 && lines[body_end - 1].trim().is_empty() {
            body_end -= 1;
        }
        let step = &lines[at..body_end];
        match input_condition(step, value) {
            Some(false) => {}
            Some(true) => out.extend(
                step.iter()
                    .filter(|line| !line.starts_with(&format!("{STEP_KEY_INDENT}if: "))),
            ),
            None => out.extend(step),
        }
        out.extend(&lines[body_end..end]);
        at = end;
    }
    let mut settled = out.join("\n");
    settled.push('\n');
    settled
}

/// The expression of the GitHub unit that gives a job the token of the workflow for the GitHub
/// tracker only.
const TOKEN_EXPRESSION: &str = "${{ inputs.tracker == 'github' && github.token || '' }}";

/// The index of the first line of `lines` that `matches`.
fn line_index(lines: &[&str], what: &str, matches: impl Fn(&str) -> bool) -> usize {
    lines
        .iter()
        .position(|line| matches(line))
        .unwrap_or_else(|| panic!("the unit holds no {what} line"))
}

/// The inline form for GitHub: the workflow of the unit from its `name` on, triggered by the
/// requests and the pushes to the default branch `branch` of the project instead of a call, with
/// every input bound for the install. The install of the form is the `release` of this binary, with
/// no setup command and no validation capture.
pub(super) fn github_inline(wiring: &Wiring<'_>, unit: &str, branch: &str) -> String {
    let lines: Vec<&str> = unit.lines().collect();
    let name = line_index(&lines, "name", |line| line.starts_with("name:"));
    let on = line_index(&lines, "on", |line| line == "on:");
    let env = line_index(&lines, "env", |line| line == "env:");
    let mut text = String::new();
    for line in &lines[name..on] {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(&github_triggers(branch));
    text.push('\n');
    for line in &lines[env..] {
        text.push_str(line);
        text.push('\n');
    }
    let value = |name: &str| -> String {
        match name {
            "dir" => wiring.path.to_string(),
            "key" => wiring.key.to_string(),
            "version" => wiring.version.to_string(),
            "install" | "floor-install" => "release".to_string(),
            "setup" | "validation" => String::new(),
            "tracker" => wiring.tracker.unwrap_or_default().to_string(),
            other => panic!("the unit reads the input {other}, which ci does not bind"),
        }
    };
    let token = if value("tracker") == "github" {
        "${{ github.token }}"
    } else {
        "\"\""
    };
    let text = settle_steps(&text, &value).replace(TOKEN_EXPRESSION, token);
    bind(&text, ("${{", "}}"), "inputs.", &value)
}

/// The default of the input `name` in the `spec` header `spec` of the GitLab unit.
fn spec_default(spec: &str, name: &str) -> String {
    let documents = Yaml::load_from_str(spec).expect("the spec header of the unit is YAML");
    documents
        .first()
        .and_then(|document| document.as_mapping_get("spec"))
        .and_then(|node| node.as_mapping_get("inputs"))
        .and_then(|node| node.as_mapping_get(name))
        .and_then(|node| node.as_mapping_get("default"))
        .and_then(Yaml::as_str)
        .unwrap_or_else(|| panic!("the unit gives the input {name} a default"))
        .to_string()
}

/// The inline form for GitLab: the jobs of the unit without its `spec` header and the annotation of
/// its release version line, every input bound for the install. The token environment is
/// `houserules-<key>`, the default of the unit.
pub(super) fn gitlab_inline(wiring: &Wiring<'_>, unit: &str) -> String {
    let (spec, jobs) = unit
        .split_once("\n---\n")
        .expect("the unit has a spec header");
    let value = |name: &str| -> String {
        match name {
            "dir" => wiring.path.to_string(),
            "key" => wiring.key.to_string(),
            "install" | "floor-install" => "release".to_string(),
            "setup" | "validation" => String::new(),
            "tracker" => wiring.tracker.unwrap_or_default().to_string(),
            "token-environment" => format!("houserules-{}", wiring.key),
            "image" | "stage" => spec_default(spec, name),
            other => panic!("the unit reads the input {other}, which ci does not bind"),
        }
    };
    bind(
        &jobs.replace(RELEASE_ANNOTATION, ""),
        ("$[[", "]]"),
        "inputs.",
        &value,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The GitHub unit that the binary embeds.
    const GITHUB_UNIT: &str =
        include_str!("../../../../../.github/workflows/houserules-review.yml");

    /// The GitLab unit that the binary embeds.
    const GITLAB_UNIT: &str = include_str!("../../../../../ci/gitlab-review.yml");

    fn wiring<'a>(path: &'a str, key: &'a str, tracker: Option<&'a str>) -> Wiring<'a> {
        Wiring {
            version: "2.0.0",
            path,
            key,
            tracker,
        }
    }

    /// No form of the embedded units keeps an input, whatever the install, and the units read no
    /// input that the forms do not bind (the binding panics on one).
    #[test]
    fn the_forms_of_the_embedded_units_bind_every_input() {
        for wiring in [
            wiring(".", "root", None),
            wiring("a b/c'd", "a-b-c-d", Some("github")),
            wiring("sub/dir", "sub-dir", Some("beads")),
        ] {
            let github = github_inline(&wiring, GITHUB_UNIT, "main");
            assert!(!github.contains("inputs."), "{github}");
            assert!(!github.contains("workflow_call"));
            let gitlab = gitlab_inline(&wiring, GITLAB_UNIT);
            assert!(!gitlab.contains("inputs."), "{gitlab}");
            assert!(!gitlab.contains("spec:"));
        }
    }

    /// An expression that is one input is bound by the JSON string of its value as a whole value,
    /// and by the raw value inside a name.
    #[test]
    fn a_whole_value_is_a_json_string_and_a_name_holds_the_raw_key() {
        let value = |name: &str| format!("<{name}>");
        let text =
            "  dir: ${{ inputs.dir }}\n  name: x-${{ inputs.key }}-y\n  tail: ${{ inputs.dir }}";
        assert_eq!(
            bind(text, ("${{", "}}"), "inputs.", &value),
            "  dir: \"<dir>\"\n  name: x-<key>-y\n  tail: \"<dir>\""
        );
    }

    /// An input among other terms of an expression gets a string literal, and a quote in the value
    /// is doubled.
    #[test]
    fn an_input_among_other_terms_gets_a_string_literal() {
        let value = |name: &str| format!("it's {name}");
        let text = "if: ${{ inputs.tracker == 'github' && inputs.floor-install != '' }}";
        assert_eq!(
            bind(text, ("${{", "}}"), "inputs.", &value),
            "if: ${{ 'it''s tracker' == 'github' && 'it''s floor-install' != '' }}"
        );
    }

    /// An expression with no input stays as it is.
    #[test]
    fn an_expression_with_no_input_stays() {
        let value = |_: &str| String::new();
        let text = "path: ${{ runner.temp }}/x\nif: ${{ github.event_name == 'push' }}";
        assert_eq!(bind(text, ("${{", "}}"), "inputs.", &value), text);
    }

    /// The reference forms name the version, the install, and the tracker.
    #[test]
    fn the_reference_forms_name_the_version_the_install_and_the_tracker() {
        let wiring = wiring("a\"b", "a-b", Some("beads"));
        let github = github_reference(&wiring, Some("abc"), "main");
        assert!(github.contains("@abc # v2.0.0\n"), "{github}");
        assert!(github.contains("      dir: \"a\\\"b\"\n"), "{github}");
        assert!(github.ends_with("      tracker: beads\n"), "{github}");
        assert!(!github.contains("Resolve"), "a pinned caller needs no hint");
        let gitlab = gitlab_reference(&wiring, "sha256-x");
        assert!(
            gitlab.contains("/v2.0.0/ci/gitlab-review.yml\n"),
            "{gitlab}"
        );
        assert!(gitlab.contains("    integrity: sha256-x\n"), "{gitlab}");
    }
}

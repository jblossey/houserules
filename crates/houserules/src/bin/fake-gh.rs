//! `fake-gh`: a test double of the `gh` CLI for the GitHub adapter (batch 34,
//! spec 3.6). It serves the calls that the adapter makes from the JSON store
//! that `FAKE_GH_STORE` names, and a test points `HOUSERULES_GH` at it. CI
//! never writes to a GitHub repository.
//!
//! The calls it serves:
//!
//! - `gh api [--paginate] repos/<repo>/issues|labels|milestones[?query]`: the
//!   rows as GitHub returns them, newest first. With `--paginate` every page
//!   comes back as one array, as `gh` prints it; without it, only the page
//!   that `page` and `per_page` name.
//! - `gh api repos/<repo>/milestones -f title=<t>`: creates a milestone.
//! - `gh issue create|edit|close|reopen --repo <repo> ...`: the writes that the
//!   adapter makes. A label or milestone that does not exist is refused, as
//!   `gh` refuses it; closing an issue that is closed changes nothing.
//! - `gh label create <name> --repo <repo>`.
//!
//! Any other call, or an unknown flag, is a named fault with exit 1. The store
//! is one JSON object: `repo` (the one repository it serves; another is a 404),
//! `auth` (`false` makes every call exit 4, as `gh` does without a login),
//! `failure` (a `{code, stderr}` object that makes every call fail), `issues`
//! (the REST shape), `labels` (names), `milestones` (`number`, `title`), and
//! `calls` (the argument list of every call, in order, kept even for a call
//! that fails).

use std::env;
use std::fs;
use std::process::ExitCode;

use serde_json::{Value, json};

/// What a call prints and the exit code it ends with.
struct Reply {
    code: u8,
    stdout: String,
    stderr: String,
}

impl Reply {
    /// A call that succeeds and prints `stdout`.
    fn ok(stdout: impl Into<String>) -> Self {
        Self {
            code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    /// A call that ends with `code` and prints `stderr`.
    fn fail(code: u8, stderr: impl Into<String>) -> Self {
        Self {
            code,
            stdout: String::new(),
            stderr: format!("{}\n", stderr.into()),
        }
    }
}

/// The flags that take a value.
const VALUE_FLAGS: [&str; 12] = [
    "--repo",
    "--title",
    "--body",
    "--label",
    "--milestone",
    "--add-label",
    "--remove-label",
    "--reason",
    "-f",
    "--raw-field",
    "-X",
    "--method",
];
/// The switches, which take no value.
const SWITCHES: [&str; 1] = ["--paginate"];

/// The arguments of one call, split into flags and positional words.
#[derive(Default)]
struct Args {
    flags: Vec<(String, String)>,
    paginate: bool,
    positional: Vec<String>,
}

impl Args {
    /// Splits `args`: a flag with a value, a switch, or a positional word; `--`
    /// makes every word after it positional.
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut parsed = Self::default();
        let mut words = args.iter();
        while let Some(word) = words.next() {
            if word == "--" {
                parsed.positional.extend(words.by_ref().cloned());
            } else if VALUE_FLAGS.contains(&word.as_str()) {
                let value = words
                    .next()
                    .ok_or_else(|| format!("fake-gh: {word} needs a value"))?;
                parsed.flags.push((word.clone(), value.clone()));
            } else if SWITCHES.contains(&word.as_str()) {
                parsed.paginate = true;
            } else if word.starts_with('-') {
                return Err(format!("fake-gh: unsupported flag {word}"));
            } else {
                parsed.positional.push(word.clone());
            }
        }
        Ok(parsed)
    }

    /// The values of the flag `name`, in order.
    fn values(&self, name: &str) -> Vec<&str> {
        self.flags
            .iter()
            .filter(|(flag, _)| flag == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// The last value of the flag `name`.
    fn value(&self, name: &str) -> Option<&str> {
        self.values(name).last().copied()
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(path) = env::var_os("FAKE_GH_STORE") else {
        eprintln!("fake-gh: FAKE_GH_STORE is not set");
        return ExitCode::from(1);
    };
    let mut store: Value = match fs::read_to_string(&path)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
    {
        Ok(store) => store,
        Err(error) => {
            eprintln!("fake-gh: cannot read the store: {error}");
            return ExitCode::from(1);
        }
    };
    store["calls"]
        .as_array_mut()
        .expect("the store has a calls list")
        .push(json!(args));
    let reply = serve(&mut store, &args);
    let text = serde_json::to_string_pretty(&store).expect("the store serializes");
    if let Err(error) = fs::write(&path, text) {
        eprintln!("fake-gh: cannot write the store: {error}");
        return ExitCode::from(1);
    }
    print!("{}", reply.stdout);
    eprint!("{}", reply.stderr);
    ExitCode::from(reply.code)
}

/// Serves one call against `store`.
fn serve(store: &mut Value, args: &[String]) -> Reply {
    if let Some(failure) = store["failure"].as_object() {
        let code = failure["code"].as_u64().unwrap_or(1);
        let stderr = failure["stderr"].as_str().unwrap_or_default();
        return Reply {
            code: u8::try_from(code).unwrap_or(1),
            stdout: String::new(),
            stderr: stderr.to_string(),
        };
    }
    if store["auth"] == json!(false) {
        return Reply::fail(
            4,
            "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.",
        );
    }
    let parsed = match Args::parse(args) {
        Ok(parsed) => parsed,
        Err(fault) => return Reply::fail(1, fault),
    };
    match parsed.positional.first().map(String::as_str) {
        Some("api") => api(store, &parsed),
        Some("issue") => issue(store, &parsed),
        Some("label") => label(store, &parsed),
        _ => Reply::fail(1, format!("fake-gh: unsupported call {args:?}")),
    }
}

/// `true` when `repo` is the repository that the store serves.
fn serves(store: &Value, repo: &str) -> bool {
    store["repo"].as_str() == Some(repo)
}

/// The refusal of a repository that the store does not serve, in the words of
/// `gh issue` and `gh label`.
fn no_such_repository(repo: &str) -> Reply {
    Reply::fail(
        1,
        format!("GraphQL: Could not resolve to a Repository with the name '{repo}'. (repository)"),
    )
}

/// Serves `gh api`.
fn api(store: &mut Value, args: &Args) -> Reply {
    let Some(endpoint) = args.positional.get(1) else {
        return Reply::fail(1, "fake-gh: gh api needs an endpoint");
    };
    let (path, query) = endpoint.split_once('?').unwrap_or((endpoint, ""));
    let parts: Vec<&str> = path.split('/').collect();
    let [prefix, owner, name, collection] = parts[..] else {
        return Reply::fail(1, format!("fake-gh: unsupported endpoint {endpoint}"));
    };
    if prefix != "repos" {
        return Reply::fail(1, format!("fake-gh: unsupported endpoint {endpoint}"));
    }
    if !serves(store, &format!("{owner}/{name}")) {
        return Reply::fail(1, "gh: Not Found (HTTP 404)");
    }
    let field = |key: &str| {
        query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.to_string())
    };
    let creating = !args.values("-f").is_empty() || !args.values("--raw-field").is_empty();
    match (collection, creating) {
        ("issues", false) => {
            let state = field("state").unwrap_or_else(|| "open".to_string());
            let mut rows: Vec<Value> = store["issues"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|issue| state == "all" || issue["state"].as_str() == Some(state.as_str()))
                .collect();
            rows.sort_by_key(|issue| std::cmp::Reverse(issue["number"].as_u64()));
            page(rows, args.paginate, field("page"), field("per_page"))
        }
        ("labels", false) => {
            let rows = store["labels"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|name| json!({"name": name}))
                .collect();
            page(rows, args.paginate, field("page"), field("per_page"))
        }
        ("milestones", false) => {
            let rows = store["milestones"].as_array().cloned().unwrap_or_default();
            page(rows, args.paginate, field("page"), field("per_page"))
        }
        ("milestones", true) => create_milestone(store, args),
        _ => Reply::fail(1, format!("fake-gh: unsupported endpoint {endpoint}")),
    }
}

/// The rows that a list call returns: all of them with `--paginate`, else the
/// page that `page` and `per_page` name (GitHub's defaults are page 1 and 30
/// rows).
fn page(rows: Vec<Value>, paginate: bool, page: Option<String>, per_page: Option<String>) -> Reply {
    let rows = if paginate {
        rows
    } else {
        let size = per_page.and_then(|size| size.parse().ok()).unwrap_or(30);
        let number: usize = page.and_then(|page| page.parse().ok()).unwrap_or(1);
        rows.into_iter()
            .skip(size * number.saturating_sub(1))
            .take(size)
            .collect()
    };
    Reply::ok(format!("{}\n", Value::Array(rows)))
}

/// Serves `gh api repos/<repo>/milestones -f title=<t>`.
fn create_milestone(store: &mut Value, args: &Args) -> Reply {
    let title = args
        .values("-f")
        .into_iter()
        .chain(args.values("--raw-field"))
        .find_map(|field| field.strip_prefix("title="));
    let Some(title) = title else {
        return Reply::fail(1, "fake-gh: a milestone needs title=<t>");
    };
    let milestones = store["milestones"]
        .as_array_mut()
        .expect("the store has milestones");
    if milestones
        .iter()
        .any(|milestone| milestone["title"] == json!(title))
    {
        return Reply::fail(1, "gh: Validation Failed (HTTP 422)");
    }
    let number = milestones.len() + 1;
    let milestone = json!({"number": number, "title": title});
    milestones.push(milestone.clone());
    Reply::ok(format!("{milestone}\n"))
}

/// Serves `gh issue create|edit|close|reopen`.
fn issue(store: &mut Value, args: &Args) -> Reply {
    let repo = args.value("--repo").unwrap_or_default();
    if !serves(store, repo) {
        return no_such_repository(repo);
    }
    match args.positional.get(1).map(String::as_str) {
        Some("create") => create_issue(store, args, repo),
        Some(verb @ ("edit" | "close" | "reopen")) => {
            let Some(number) = args
                .positional
                .get(2)
                .and_then(|word| word.parse::<u64>().ok())
            else {
                return Reply::fail(1, format!("fake-gh: gh issue {verb} needs an issue number"));
            };
            let Some(position) = store["issues"].as_array().and_then(|issues| {
                issues
                    .iter()
                    .position(|issue| issue["number"] == json!(number))
            }) else {
                return Reply::fail(
                    1,
                    format!(
                        "GraphQL: Could not resolve to an issue or pull request with the number of {number}. (repository.issue)"
                    ),
                );
            };
            match verb {
                "edit" => edit_issue(store, position, args, repo, number),
                "close" => close_issue(store, position, args, number),
                _ => reopen_issue(store, position, number),
            }
        }
        _ => Reply::fail(1, "fake-gh: unsupported gh issue call"),
    }
}

/// The refusal of a label that the store does not hold, or of a milestone.
fn missing_labels_or_milestone(
    store: &Value,
    labels: &[&str],
    milestone: Option<&str>,
) -> Option<Reply> {
    let known = store["labels"].as_array().cloned().unwrap_or_default();
    if let Some(label) = labels.iter().find(|label| !known.contains(&json!(label))) {
        return Some(Reply::fail(
            1,
            format!("could not add label: '{label}' not found"),
        ));
    }
    let milestone = milestone?;
    let known = store["milestones"].as_array().cloned().unwrap_or_default();
    (!known.iter().any(|known| known["title"] == json!(milestone))).then(|| {
        Reply::fail(
            1,
            format!("could not add to milestone '{milestone}': '{milestone}' not found"),
        )
    })
}

/// Serves `gh issue create`: the number is one above the highest, and the URL
/// is the one line it prints.
fn create_issue(store: &mut Value, args: &Args, repo: &str) -> Reply {
    let Some(title) = args.value("--title") else {
        return Reply::fail(1, "fake-gh: gh issue create needs --title");
    };
    let labels = args.values("--label");
    let milestone = args.value("--milestone");
    if let Some(refusal) = missing_labels_or_milestone(store, &labels, milestone) {
        return refusal;
    }
    let issues = store["issues"]
        .as_array_mut()
        .expect("the store has issues");
    let number = issues
        .iter()
        .filter_map(|issue| issue["number"].as_u64())
        .max()
        .unwrap_or(0)
        + 1;
    issues.push(json!({
        "number": number,
        "title": title,
        "body": args.value("--body").unwrap_or_default(),
        "state": "open",
        "state_reason": null,
        "labels": labels.iter().map(|name| json!({"name": name})).collect::<Vec<_>>(),
        "milestone": milestone.map(|title| json!({"title": title, "number": 1})),
    }));
    Reply::ok(format!("https://github.com/{repo}/issues/{number}\n"))
}

/// Serves `gh issue edit`: labels to add and remove, a milestone, a title, a
/// body.
fn edit_issue(store: &mut Value, position: usize, args: &Args, repo: &str, number: u64) -> Reply {
    let added = args.values("--add-label");
    let milestone = args.value("--milestone");
    if let Some(refusal) = missing_labels_or_milestone(store, &added, milestone) {
        return refusal;
    }
    let issue = &mut store["issues"][position];
    let mut labels: Vec<Value> = issue["labels"].as_array().cloned().unwrap_or_default();
    labels.retain(|label| {
        !args
            .values("--remove-label")
            .contains(&label["name"].as_str().unwrap_or_default())
    });
    for name in added {
        if !labels.iter().any(|label| label["name"] == json!(name)) {
            labels.push(json!({"name": name}));
        }
    }
    issue["labels"] = Value::Array(labels);
    if let Some(title) = milestone {
        issue["milestone"] = json!({"title": title, "number": 1});
    }
    if let Some(title) = args.value("--title") {
        issue["title"] = json!(title);
    }
    if let Some(body) = args.value("--body") {
        issue["body"] = json!(body);
    }
    Reply::ok(format!("https://github.com/{repo}/issues/{number}\n"))
}

/// Serves `gh issue close`: an issue that is closed stays as it is, as in
/// `gh`.
fn close_issue(store: &mut Value, position: usize, args: &Args, number: u64) -> Reply {
    let reason = match args.value("--reason") {
        None | Some("completed") => "completed",
        Some("not planned") => "not_planned",
        Some("duplicate") => "duplicate",
        Some(other) => return Reply::fail(1, format!("invalid close reason {other:?}")),
    };
    let issue = &mut store["issues"][position];
    if issue["state"] == json!("closed") {
        return Reply {
            code: 0,
            stdout: String::new(),
            stderr: format!("! Issue #{number} is already closed\n"),
        };
    }
    issue["state"] = json!("closed");
    issue["state_reason"] = json!(reason);
    Reply::ok(String::new())
}

/// Serves `gh issue reopen`.
fn reopen_issue(store: &mut Value, position: usize, number: u64) -> Reply {
    let issue = &mut store["issues"][position];
    if issue["state"] == json!("closed") {
        issue["state"] = json!("open");
        issue["state_reason"] = json!("reopened");
        return Reply::ok(String::new());
    }
    Reply {
        code: 0,
        stdout: String::new(),
        stderr: format!("! Issue #{number} is already open\n"),
    }
}

/// Serves `gh label create <name>`.
fn label(store: &mut Value, args: &Args) -> Reply {
    let repo = args.value("--repo").unwrap_or_default();
    if !serves(store, repo) {
        return no_such_repository(repo);
    }
    let (Some("create"), Some(name)) = (
        args.positional.get(1).map(String::as_str),
        args.positional.get(2),
    ) else {
        return Reply::fail(1, "fake-gh: unsupported gh label call");
    };
    let labels = store["labels"]
        .as_array_mut()
        .expect("the store has labels");
    if labels.contains(&json!(name)) {
        return Reply::fail(
            1,
            format!(
                "label with name \"{name}\" already exists; use `--force` to update its color and description"
            ),
        );
    }
    labels.push(json!(name));
    Reply::ok(String::new())
}

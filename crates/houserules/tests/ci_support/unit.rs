//! The model of a published unit: `Host`, `Unit`, and `parse_unit`, which reads a unit
//! with `saphyr` into JSON values. A GitHub job is a key of `jobs`; a GitLab job is a
//! top-level key named `houserules-<key>-<role>`, with the keys of the hidden job it
//! extends.

use std::collections::{BTreeMap, BTreeSet};

use saphyr::{LoadableYamlNode, Scalar, Yaml};
use serde_json::{Map, Value};

/// The host that a unit is written for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Host {
    /// A GitHub reusable workflow.
    GitHub,
    /// A GitLab file for `include:remote`, read for the install with `key`: the
    /// job names carry the key (`houserules-<key>-gates`).
    GitLab {
        /// The `key` input of the install.
        key: String,
    },
}

impl Host {
    /// The GitLab host for the install with `key`.
    pub fn gitlab(key: &str) -> Self {
        Host::GitLab {
            key: key.to_string(),
        }
    }
}

/// One shell text that a job runs: a `run` script of a GitHub step or an item
/// of a GitLab `script`.
#[derive(Clone, Debug)]
pub struct Script {
    /// The name of the GitHub step, or `script` for a GitLab item.
    pub name: String,
    /// The shell text.
    pub text: String,
}

/// A published unit, read into JSON values.
#[derive(Clone)]
pub struct Unit {
    /// The host the unit is written for.
    pub host: Host,
    /// The text of the unit, with the key of a GitLab install filled in.
    pub text: String,
    /// The `spec` document of a GitLab unit (its first YAML document).
    spec: Option<Value>,
    /// The workflow of a GitHub unit, or the configuration of a GitLab unit.
    body: Value,
}

/// `node` as a JSON value. A tag or an alias is a defect of the unit.
fn to_value(node: &Yaml<'_>) -> Value {
    match node {
        Yaml::Value(Scalar::Null) => Value::Null,
        Yaml::Value(Scalar::Boolean(flag)) => Value::Bool(*flag),
        Yaml::Value(Scalar::Integer(number)) => Value::from(*number),
        Yaml::Value(Scalar::FloatingPoint(number)) => {
            serde_json::Number::from_f64(number.into_inner()).map_or(Value::Null, Value::Number)
        }
        Yaml::Value(Scalar::String(text)) => Value::String(text.to_string()),
        Yaml::Sequence(items) => Value::Array(items.iter().map(to_value).collect()),
        Yaml::Mapping(mapping) => Value::Object(
            mapping
                .iter()
                .map(|(key, value)| {
                    let name = key
                        .as_str()
                        .map_or_else(|| format!("{key:?}"), str::to_string);
                    (name, to_value(value))
                })
                .collect::<Map<String, Value>>(),
        ),
        other => panic!("the unit holds a YAML node that parse_unit does not read: {other:?}"),
    }
}

/// Reads `text`, the text of a unit for `host`, with `saphyr`. A GitLab unit
/// has its `$[[ inputs.key ]]` interpolations filled in with the key of the
/// install; every other input stays as the text of the unit holds it.
///
/// # Panics
///
/// Panics when the text is no YAML, or holds a tag or an alias.
pub fn parse_unit(text: &str, host: Host) -> Unit {
    let text = match &host {
        Host::GitHub => text.to_string(),
        Host::GitLab { key } => text.replace("$[[ inputs.key ]]", key),
    };
    let documents = Yaml::load_from_str(&text)
        .unwrap_or_else(|error| panic!("the unit is not valid YAML: {error}"));
    let mut values: Vec<Value> = documents.iter().map(to_value).collect();
    let (spec, body) = match (&host, values.len()) {
        (Host::GitHub, 1) => (None, values.remove(0)),
        (Host::GitLab { .. }, 2) => {
            let spec = values.remove(0);
            (Some(spec), values.remove(0))
        }
        (_, count) => panic!("the unit holds {count} YAML documents"),
    };
    Unit {
        host,
        text,
        spec,
        body,
    }
}

/// Reads `text`, an inline form of the unit for `host`: the jobs of the unit with every input
/// filled in, as `houserules ci print --mode inline` prints them. An inline form holds one YAML
/// document and no `spec` header, because it takes no input. On GitLab `host` names the key of the
/// install, which the job names of the form carry.
///
/// # Panics
///
/// Panics when the text is no YAML, holds a tag or an alias, or holds more than one document.
pub fn parse_inline_unit(text: &str, host: Host) -> Unit {
    let documents = Yaml::load_from_str(text)
        .unwrap_or_else(|error| panic!("the inline form is not valid YAML: {error}"));
    let [document] = documents.as_slice() else {
        panic!("the inline form holds {} YAML documents", documents.len());
    };
    Unit {
        host,
        text: text.to_string(),
        spec: None,
        body: to_value(document),
    }
}

/// The text of a scalar value that holds a string, or the `value` of a GitLab
/// variable that sets `expand: false`.
fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(map) => map.get("value").and_then(text_of),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Every string below `value`, in document order: the strings of the values and
/// the names of the keys, so that a search for a variable name finds the
/// variable that a job sets.
fn strings_below(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::String(text) => found.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| strings_below(item, found)),
        Value::Object(map) => map.iter().for_each(|(key, item)| {
            found.push(key.clone());
            strings_below(item, found);
        }),
        _ => {}
    }
}

impl Unit {
    /// A copy of the unit whose workflow or configuration `edit` has changed: a
    /// test makes one forbidden shape this way and shows that an assertion
    /// fails on it.
    pub fn mutated(&self, edit: impl FnOnce(&mut Value)) -> Unit {
        let mut copy = self.clone();
        edit(&mut copy.body);
        copy
    }

    /// The workflow of a GitHub unit, or the configuration of a GitLab unit, as JSON: the whole
    /// document that a test compares with the document it expects.
    pub fn body(&self) -> &Value {
        &self.body
    }

    /// The top-level key `key` of the workflow or of the configuration, as JSON.
    pub fn top_level(&self, key: &str) -> Option<&Value> {
        self.body.get(key)
    }

    /// The prefix of the job names of a GitLab install.
    fn prefix(&self) -> String {
        match &self.host {
            Host::GitLab { key } => format!("houserules-{key}-"),
            Host::GitHub => String::new(),
        }
    }

    /// The name of the hidden job that the jobs of a GitLab install extend.
    pub fn hidden_base_name(&self) -> String {
        match &self.host {
            Host::GitLab { key } => format!(".houserules-{key}-base"),
            Host::GitHub => panic!("a GitHub unit has no hidden job"),
        }
    }

    /// The key of a job in the unit: the role on GitHub, the prefixed name on
    /// GitLab.
    pub fn job_key(&self, role: &str) -> String {
        match self.host {
            Host::GitHub => role.to_string(),
            Host::GitLab { .. } => format!("{}{role}", self.prefix()),
        }
    }

    /// The names of the jobs of the unit as the file spells them: the keys of
    /// `jobs` on GitHub, the top-level keys that do not start with a dot on
    /// GitLab.
    pub fn job_keys(&self) -> Vec<String> {
        let map = match self.host {
            Host::GitHub => self.body.get("jobs"),
            Host::GitLab { .. } => Some(&self.body),
        };
        map.and_then(Value::as_object)
            .map(|map| {
                map.keys()
                    .filter(|key| !key.starts_with('.'))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The hidden jobs of a GitLab unit: the top-level keys with a leading dot.
    pub fn hidden_keys(&self) -> Vec<String> {
        self.body
            .as_object()
            .map(|map| {
                map.keys()
                    .filter(|key| key.starts_with('.'))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The job of `role` as the file holds it, before an `extends` is applied.
    fn raw_job(&self, role: &str) -> Value {
        let key = self.job_key(role);
        let jobs = match self.host {
            Host::GitHub => self.body.get("jobs"),
            Host::GitLab { .. } => Some(&self.body),
        };
        jobs.and_then(|jobs| jobs.get(&key))
            .cloned()
            .unwrap_or_else(|| panic!("the unit holds no job {key}"))
    }

    /// The job of `role`. A GitLab job gets the keys of the hidden job that it
    /// extends: the `variables` merge by name, and every other key of the job
    /// replaces the key of the hidden job.
    pub fn job(&self, role: &str) -> Value {
        let job = self.raw_job(role);
        let Some(extends) = job.get("extends").and_then(Value::as_str) else {
            return job;
        };
        let base =
            self.body.get(extends).cloned().unwrap_or_else(|| {
                panic!("the job {role} extends {extends}, which the unit lacks")
            });
        let (Value::Object(mut merged), Value::Object(own)) = (base, job) else {
            panic!("the job {role} and the job it extends are mappings");
        };
        for (key, value) in own {
            if key == "extends" {
                continue;
            }
            match (merged.get_mut(&key), value) {
                (Some(Value::Object(base_map)), Value::Object(own_map)) if key == "variables" => {
                    base_map.extend(own_map);
                }
                (_, value) => {
                    merged.insert(key, value);
                }
            }
        }
        Value::Object(merged)
    }

    /// The roles of the jobs that the unit holds, found by name.
    pub fn roles(&self) -> BTreeSet<String> {
        let prefix = self.prefix();
        self.job_keys()
            .into_iter()
            .map(|key| key.strip_prefix(&prefix).unwrap_or(&key).to_string())
            .collect()
    }

    /// Every shell text that the job of `role` runs: the `run` of each GitHub
    /// step and each item of `before_script`, `script`, and `after_script` of a
    /// GitLab job.
    pub fn scripts(&self, role: &str) -> Vec<Script> {
        let job = self.job(role);
        match self.host {
            Host::GitHub => job
                .get("steps")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|step| {
                    let text = step.get("run")?.as_str()?.to_string();
                    let name = step
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("run")
                        .to_string();
                    Some(Script { name, text })
                })
                .collect(),
            Host::GitLab { .. } => ["before_script", "script", "after_script"]
                .iter()
                .filter_map(|key| job.get(*key))
                .flat_map(|value| {
                    let mut texts = Vec::new();
                    strings_below(value, &mut texts);
                    texts
                })
                .map(|text| Script {
                    name: "script".to_string(),
                    text,
                })
                .collect(),
        }
    }

    /// The shell text of the step `name` of the GitHub job `role`.
    ///
    /// # Panics
    ///
    /// Panics when the job holds no run step of that name.
    pub fn step_script(&self, role: &str, name: &str) -> String {
        self.scripts(role)
            .into_iter()
            .find(|script| script.name == name)
            .unwrap_or_else(|| panic!("the job {role} holds no run step named {name}"))
            .text
    }

    /// The whole script of a GitLab job, its items joined by a line break.
    pub fn job_script(&self, role: &str) -> String {
        self.scripts(role)
            .into_iter()
            .map(|script| script.text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The step `name` of the GitHub job `role`, as JSON.
    pub fn step(&self, role: &str, name: &str) -> Value {
        self.job(role)
            .get("steps")
            .and_then(Value::as_array)
            .and_then(|steps| {
                steps
                    .iter()
                    .find(|step| step.get("name").and_then(Value::as_str) == Some(name))
            })
            .cloned()
            .unwrap_or_else(|| panic!("the job {role} holds no step named {name}"))
    }

    /// The variable `name` of the job `role`: the `variables` of a GitLab job
    /// (after `extends`), the `env` of a GitHub job, then the `env` of the
    /// workflow. A GitLab variable that sets `expand: false` gives its `value`.
    pub fn variable(&self, role: &str, name: &str) -> Option<String> {
        let job = self.job(role);
        match self.host {
            Host::GitHub => job
                .get("env")
                .and_then(|env| env.get(name))
                .or_else(|| self.body.get("env").and_then(|env| env.get(name)))
                .and_then(text_of),
            Host::GitLab { .. } => job
                .get("variables")
                .and_then(|variables| variables.get(name))
                .and_then(text_of),
        }
    }

    /// The variables that the job `role` gets with a value that needs no
    /// expression: the workflow `env` and the job `env` on GitHub, the
    /// `variables` of the job (after `extends`) on GitLab. A value that holds
    /// an expression (`${{ }}` or `$[[ ]]`) is left out, because only a runner
    /// can evaluate it. A test supplies those values.
    pub fn static_env(&self, role: &str) -> Vec<(String, String)> {
        let job = self.job(role);
        let mut pairs: BTreeMap<String, String> = BTreeMap::new();
        let mut collect = |value: Option<&Value>| {
            for (name, value) in value.and_then(Value::as_object).into_iter().flatten() {
                if let Some(text) = text_of(value)
                    && !text.contains("${{")
                    && !text.contains("$[[")
                {
                    pairs.insert(name.clone(), text);
                }
            }
        };
        match self.host {
            Host::GitHub => {
                collect(self.body.get("env"));
                collect(job.get("env"));
            }
            Host::GitLab { .. } => collect(job.get("variables")),
        }
        pairs.into_iter().collect()
    }

    /// The variable `name` of the `env` of the step `step` of the GitHub job
    /// `role`.
    pub fn step_variable(&self, role: &str, step: &str, name: &str) -> Option<String> {
        self.step(role, step)
            .get("env")
            .and_then(|env| env.get(name))
            .and_then(text_of)
    }

    /// The `permissions` of the GitHub job `role`.
    pub fn permissions(&self, role: &str) -> BTreeMap<String, String> {
        self.job(role)
            .get("permissions")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("the job {role} holds no permissions mapping"))
            .iter()
            .map(|(scope, value)| {
                (
                    scope.clone(),
                    text_of(value).unwrap_or_else(|| panic!("permission {scope} is no text")),
                )
            })
            .collect()
    }

    /// The names of the inputs of the unit.
    pub fn input_names(&self) -> BTreeSet<String> {
        let inputs = match self.host {
            Host::GitHub => self
                .body
                .get("on")
                .and_then(|on| on.get("workflow_call"))
                .and_then(|call| call.get("inputs")),
            Host::GitLab { .. } => self
                .spec
                .as_ref()
                .and_then(|spec| spec.get("spec"))
                .and_then(|spec| spec.get("inputs")),
        };
        inputs
            .and_then(Value::as_object)
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// The input `name`, as JSON.
    pub fn input(&self, name: &str) -> Value {
        let inputs = match self.host {
            Host::GitHub => self
                .body
                .get("on")
                .and_then(|on| on.get("workflow_call"))
                .and_then(|call| call.get("inputs")),
            Host::GitLab { .. } => self
                .spec
                .as_ref()
                .and_then(|spec| spec.get("spec"))
                .and_then(|spec| spec.get("inputs")),
        };
        inputs
            .and_then(|inputs| inputs.get(name))
            .cloned()
            .unwrap_or_else(|| panic!("the unit holds no input {name}"))
    }

    /// Every string of the job `role`, after `extends`, joined by line breaks.
    /// A search for a name in this text finds every use of it in the job.
    pub fn job_text(&self, role: &str) -> String {
        let mut found = Vec::new();
        strings_below(&self.job(role), &mut found);
        found.join("\n")
    }

    /// The `uses` value of every step of the GitHub job `role`.
    pub fn uses(&self, role: &str) -> Vec<String> {
        self.job(role)
            .get("steps")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|step| step.get("uses").and_then(Value::as_str).map(str::to_string))
            .collect()
    }

    /// The name of the environment that the GitLab job `role` names under
    /// `environment`, with the `$HOUSERULES_KEY` of the install filled in, and
    /// its `action`.
    pub fn environment(&self, role: &str) -> Option<(String, String)> {
        let job = self.job(role);
        let environment = job.get("environment")?;
        let name = environment.get("name").and_then(Value::as_str)?;
        let key = self.variable(role, "HOUSERULES_KEY").unwrap_or_default();
        let action = environment
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("start");
        Some((
            name.replace("$HOUSERULES_KEY", &key).replace(
                "$[[ inputs.token-environment ]]",
                &format!("houserules-{key}"),
            ),
            action.to_string(),
        ))
    }
}

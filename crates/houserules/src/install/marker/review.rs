//! The `review` key of `.houserules.json` (batch 37, spec 6.1) and the keys
//! that the CI wiring adds: `review.ci.files` and `reviewLayerOffered`.
//!
//! The project writes `review`; only [`Marker::add_ci_file`] changes it, and
//! every other writer keeps it as found. [`Marker::review`] reads the whole key
//! into a [`ReviewConfig`], or gives one line for each fault, which
//! `check-knowledge` prints. A line names the marker and the key at fault. The
//! faults come in the order of the sections of the key (`diffExclude`, `route`,
//! `bundle`, `intent`, `sessions`, `ci`, `record`), not in the order of the file. Inside
//! `route` they come in this order: `default`, `warn`, each area in the order
//! of the file, `paths`, the scores, `sampleRate`, and the line that
//! `sampleRate` is needed. A key that the key list of spec 6.1 does not name is
//! no fault: the marker keeps it. `review.ci.files` is checked and not stored:
//! the change analysis reads it from the marker at each commit.
//!
//! [`Marker::review_in_areas`] also gives a line for each area of
//! `review.route.areas` that the project does not define (spec 6.1), before the
//! other lines of that area. A share, a rate, and a score lie between 0 and 1.
//!
//! A change is `auto-merge-eligible` only through an area or a path entry
//! (design.md 257): `review.route.default` and `review.route.warn` take
//! `owner-review` or `senior-review`. A project that sets an area or a path
//! entry at `auto-merge-eligible` must also set `review.route.sampleRate`,
//! because the sample of the change record needs it (spec 8.5), and both
//! limits of that area or entry.

use globset::{Glob, GlobMatcher};
use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use super::Marker;
use crate::published::route::Route;
use crate::rules::compile;

/// The intent sources that `review.intent.sources` may name: the adapters of
/// this binary (spec 4.9).
pub(crate) const ADAPTERS: &[&str] = &["work-item", "file", "openspec"];

/// An intent adapter: one entry of [`ADAPTERS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Adapter {
    /// The work items of the range, with their titles.
    WorkItem,
    /// Files of the tree at the head.
    File,
    /// The changes of an OpenSpec root in the tree at the head.
    OpenSpec,
}

impl Adapter {
    /// Every adapter, in the order of [`ADAPTERS`].
    pub(crate) const ALL: [Adapter; 3] = [Adapter::WorkItem, Adapter::File, Adapter::OpenSpec];

    /// The name that config, the bundle, and the packet use for the adapter.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Adapter::WorkItem => "work-item",
            Adapter::File => "file",
            Adapter::OpenSpec => "openspec",
        }
    }

    /// The adapter that `source` names.
    pub(crate) fn named(source: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|adapter| adapter.name() == source)
    }
}

/// The line for an intent source that is no adapter of this binary: the
/// wording of the check of `review.intent.sources`, and of the line for a
/// named intent (spec 4.9).
pub(crate) fn no_intent_source(source: &str) -> String {
    format!(
        "{source:?} is no intent source; use one of: {}",
        ADAPTERS.join(", ")
    )
}

/// The trailer key of the sessions while `review.sessions.trailers` is not set
/// (batch 38, spec 5.1): the tool Entire links a commit to a session with it.
pub(crate) const DEFAULT_SESSION_TRAILER: &str = "Entire-Checkpoint";

/// The first line of a file that `houserules ci write` created (spec 10.3).
/// The change analysis and `update` find the files of `ci write` by it.
pub(crate) const CI_WRITTEN_BY_LINE: &str = "# Written by houserules ci write; rewrite it with houserules ci write --force and the flags that wrote it.";

/// The size of a change record in bytes while `review.record.maxBytes` is not
/// set (spec 8.4).
const DEFAULT_MAX_BYTES: u64 = 65_536;

/// The smallest `review.record.maxBytes`: the minimal record fits it (spec
/// 8.4).
const MIN_MAX_BYTES: u64 = 8_192;

/// The three routes, from the lowest to the highest.
const ROUTES: [Route; 3] = [
    Route::AutoMergeEligible,
    Route::OwnerReview,
    Route::SeniorReview,
];

/// The `review` key of the marker, read: every key has its value or its
/// default.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ReviewConfig {
    /// The globs of the paths that leave the diff size (`review.diffExclude`).
    pub(crate) diff_exclude: Vec<String>,
    /// The route settings (`review.route`).
    pub(crate) route: RouteConfig,
    /// The settings of the evidence bundle check (`review.bundle`).
    pub(crate) bundle: BundleConfig,
    /// The intent sources and files (`review.intent`).
    pub(crate) intent: IntentConfig,
    /// The trailers that name a session (`review.sessions`).
    pub(crate) sessions: SessionsConfig,
    /// The size budget of a change record in bytes (`review.record.maxBytes`).
    pub(crate) max_bytes: u64,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            diff_exclude: Vec::new(),
            route: RouteConfig::default(),
            bundle: BundleConfig::default(),
            intent: IntentConfig::default(),
            sessions: SessionsConfig::default(),
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

/// The `review.sessions` settings (batch 38, spec 5.1 and 5.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionsConfig {
    /// The keys of the trailers whose value is a session reference, each once,
    /// as the first spelling of the list; a key matches without regard to its
    /// case. [`DEFAULT_SESSION_TRAILER`] while the key is not set; the project's
    /// empty list stays empty.
    pub(crate) trailers: Vec<String>,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            trailers: vec![DEFAULT_SESSION_TRAILER.to_string()],
        }
    }
}

/// The `review.route` settings.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct RouteConfig {
    /// The route of a change with no changed path and no changed rule source,
    /// of a path in no area, and of an area with no route of its own. Never
    /// `auto-merge-eligible`. `owner-review` while the key is not set.
    pub(crate) default: Route,
    /// The route of a warned row. Never `auto-merge-eligible`. `owner-review`
    /// while the key is not set.
    pub(crate) warn: Route,
    /// The settings of each area, in the order of the file.
    pub(crate) areas: IndexMap<String, AreaRoute>,
    /// The routes of the paths that a glob matches, in the order of the file.
    pub(crate) paths: Vec<PathRoute>,
    /// The score from which a change needs the owner.
    pub(crate) owner_score: Option<f64>,
    /// The score from which a change needs a senior reviewer.
    pub(crate) senior_score: Option<f64>,
    /// The share of auto-merge changes that the post-merge audit samples.
    pub(crate) sample_rate: Option<f64>,
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            default: Route::OwnerReview,
            warn: Route::OwnerReview,
            areas: IndexMap::new(),
            paths: Vec::new(),
            owner_score: None,
            senior_score: None,
            sample_rate: None,
        }
    }
}

/// The settings of one area in `review.route.areas`.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct AreaRoute {
    /// The route of the area; the default route while the key is not set.
    pub(crate) route: Option<Route>,
    /// The diff size from which an `auto-merge-eligible` area needs the owner.
    pub(crate) max_diff_lines: Option<u64>,
    /// The uncovered share from which an `auto-merge-eligible` area needs the
    /// owner.
    pub(crate) max_uncovered_share: Option<f64>,
}

/// One entry of `review.route.paths`.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct PathRoute {
    /// The glob of the paths that the entry routes.
    pub(crate) glob: String,
    /// The route of a path that the glob matches.
    pub(crate) route: Route,
    /// The diff size from which an `auto-merge-eligible` entry needs the owner.
    pub(crate) max_diff_lines: Option<u64>,
    /// The uncovered share from which an `auto-merge-eligible` entry needs the
    /// owner.
    pub(crate) max_uncovered_share: Option<f64>,
}

/// The `review.bundle` settings.
#[derive(Clone, Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct BundleConfig {
    /// The diff budget in lines of each risk tier, in the order of the file.
    pub(crate) budgets: IndexMap<String, u64>,
    /// The logins whose requests need no bundle.
    pub(crate) exempt_authors: Vec<String>,
    /// The globs of the branches whose requests need no bundle.
    pub(crate) exempt_branches: Vec<String>,
}

/// The `review.intent` settings in their checked form: every source is an
/// adapter, and every glob compiles. The fields are private: only the reader
/// of the marker builds a value that differs from the default, and it gives a
/// line for each source or glob that it refuses. A caller cannot drop a
/// setting without a line.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct IntentConfig {
    /// The adapters to run, in order, each once. `work-item` while the key is
    /// not set.
    sources: Vec<Adapter>,
    /// The globs of the files that the `file` adapter reads as intents.
    files: Vec<Glob>,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            sources: vec![Adapter::WorkItem],
            files: Vec::new(),
        }
    }
}

impl IntentConfig {
    /// The adapters to run, in the order of `review.intent.sources`, each
    /// once.
    pub(crate) fn adapters(&self) -> &[Adapter] {
        &self.sources
    }

    /// The matchers of the globs of `review.intent.files`.
    pub(crate) fn file_matchers(&self) -> Vec<GlobMatcher> {
        self.files.iter().map(Glob::compile_matcher).collect()
    }
}

impl Marker {
    /// The `review` key, read: the defaults while the marker has no such key,
    /// else the value of each key, or one line for each fault in the key. The
    /// read does not know which areas the project defines: use
    /// [`Marker::review_in_areas`] for the lines of an undefined area.
    pub(crate) fn review(&self) -> Result<ReviewConfig, Vec<String>> {
        self.read_review(None)
    }

    /// Like [`Marker::review`], and one line for each area of
    /// `review.route.areas` that is not in `defined_areas`, the areas of
    /// `knowledge/areas.json`. The line of an area comes before its other
    /// lines, and beside every other fault of the key.
    pub(crate) fn review_in_areas(
        &self,
        defined_areas: &[&str],
    ) -> Result<ReviewConfig, Vec<String>> {
        self.read_review(Some(defined_areas))
    }

    /// The read of [`Marker::review`] and [`Marker::review_in_areas`];
    /// `defined_areas` is `None` while the areas are not checked.
    fn read_review(&self, defined_areas: Option<&[&str]>) -> Result<ReviewConfig, Vec<String>> {
        let Some(value) = self.map.get("review") else {
            return Ok(ReviewConfig::default());
        };
        let mut reader = Reader {
            file: &self.file,
            defined_areas,
            faults: Vec::new(),
        };
        let config = reader.review(value);
        if reader.faults.is_empty() {
            Ok(config)
        } else {
            Err(reader.faults)
        }
    }

    /// Adds `path` at the end of `review.ci.files`, and the keys above it where
    /// the marker lacks them. Returns `true` when it added the path, and
    /// `false` when the list holds it already or the key has a shape that
    /// cannot hold it: the marker then stays as found, and `check-knowledge`
    /// names the fault.
    pub(crate) fn add_ci_file(&mut self, path: &str) -> bool {
        let Some(review) = self
            .map
            .entry("review")
            .or_insert_with(|| json!({}))
            .as_object_mut()
        else {
            return false;
        };
        let Some(ci) = review
            .entry("ci")
            .or_insert_with(|| json!({}))
            .as_object_mut()
        else {
            return false;
        };
        let Some(files) = ci
            .entry("files")
            .or_insert_with(|| json!([]))
            .as_array_mut()
        else {
            return false;
        };
        if files.iter().any(|file| file.as_str() == Some(path)) {
            return false;
        }
        files.push(json!(path));
        true
    }

    /// `true` when `update` offered the review layer to this install already
    /// (`reviewLayerOffered`, spec 10.7).
    pub(crate) fn review_layer_offered(&self) -> bool {
        self.map
            .get("reviewLayerOffered")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Sets `reviewLayerOffered` to `true`. The key keeps its place when it
    /// exists and goes last when it does not.
    pub(crate) fn set_review_layer_offered(&mut self) {
        self.map
            .insert("reviewLayerOffered".to_string(), json!(true));
    }
}

/// The reader of the `review` value: it collects one line for each fault.
struct Reader<'a> {
    /// How a line names the marker.
    file: &'a str,
    /// The areas of `knowledge/areas.json`, or `None` while the read does not
    /// check the areas of `review.route.areas`.
    defined_areas: Option<&'a [&'a str]>,
    /// The lines so far, in the order of the faults.
    faults: Vec<String>,
}

impl Reader<'_> {
    /// Adds the line `<file>: <key> <problem>`.
    fn fault(&mut self, key: &str, problem: &str) {
        self.faults.push(format!("{}: {key} {problem}", self.file));
    }

    /// Reads the `review` value `value`, one section after the other.
    fn review(&mut self, value: &Value) -> ReviewConfig {
        let mut config = ReviewConfig::default();
        let Some(review) = value.as_object() else {
            self.fault("review", "must be an object");
            return config;
        };
        config.diff_exclude = self.globs("review.diffExclude", review.get("diffExclude"));
        if let Some(route) = self.section("review.route", review.get("route")) {
            config.route = self.route(route);
        }
        if let Some(bundle) = self.section("review.bundle", review.get("bundle")) {
            config.bundle = self.bundle(bundle);
        }
        if let Some(intent) = self.section("review.intent", review.get("intent")) {
            config.intent = self.intent(intent);
        }
        if let Some(sessions) = self.section("review.sessions", review.get("sessions")) {
            config.sessions = self.sessions(sessions);
        }
        if let Some(ci) = self.section("review.ci", review.get("ci")) {
            // The change analysis reads `review.ci.files` from the marker at each
            // commit; this call only reports the fault.
            self.strings("review.ci.files", ci.get("files"));
        }
        if let Some(record) = self.section("review.record", review.get("record")) {
            config.max_bytes = self.max_bytes(record.get("maxBytes"));
        }
        config
    }

    /// The object under `key`, or `None` while the key is not set or its value
    /// is no object (a line then names the fault).
    fn section<'v>(
        &mut self,
        key: &str,
        value: Option<&'v Value>,
    ) -> Option<&'v Map<String, Value>> {
        let value = value?;
        let object = value.as_object();
        if object.is_none() {
            self.fault(key, "must be an object");
        }
        object
    }

    /// The `review.route` settings.
    fn route(&mut self, settings: &Map<String, Value>) -> RouteConfig {
        let mut config = RouteConfig::default();
        if let Some(default) = self.fallback_route("review.route.default", settings.get("default"))
        {
            config.default = default;
        }
        if let Some(warn) = self.fallback_route("review.route.warn", settings.get("warn")) {
            config.warn = warn;
        }
        let mut auto_merge = false;
        if let Some(areas) = self.section("review.route.areas", settings.get("areas")) {
            for (name, entry) in areas {
                let key = format!("review.route.areas.{name}");
                if self
                    .defined_areas
                    .is_some_and(|defined| !defined.contains(&name.as_str()))
                {
                    self.fault(&key, "is not an area of knowledge/areas.json");
                }
                let Some(entry) = self.section(&key, Some(entry)) else {
                    continue;
                };
                let area = self.route_entry(&key, entry, false);
                auto_merge |= area.route == Some(Route::AutoMergeEligible);
                config.areas.insert(name.clone(), area);
            }
        }
        match settings.get("paths") {
            None => {}
            Some(Value::Array(entries)) => {
                for (index, entry) in entries.iter().enumerate() {
                    let key = format!("review.route.paths[{index}]");
                    let Some(entry) = self.section(&key, Some(entry)) else {
                        continue;
                    };
                    let glob = self.path_glob(&key, entry.get("glob"));
                    let path = self.route_entry(&key, entry, true);
                    auto_merge |= path.route == Some(Route::AutoMergeEligible);
                    if let (Some(glob), Some(route)) = (glob, path.route) {
                        config.paths.push(PathRoute {
                            glob,
                            route,
                            max_diff_lines: path.max_diff_lines,
                            max_uncovered_share: path.max_uncovered_share,
                        });
                    }
                }
            }
            Some(_) => self.fault("review.route.paths", "must be an array of objects"),
        }
        config.owner_score = self.unit("review.route.ownerScore", settings.get("ownerScore"));
        config.senior_score = self.unit("review.route.seniorScore", settings.get("seniorScore"));
        config.sample_rate = self.unit("review.route.sampleRate", settings.get("sampleRate"));
        if auto_merge && !settings.contains_key("sampleRate") {
            self.fault(
                "review.route.sampleRate",
                "is needed when a route is \"auto-merge-eligible\"",
            );
        }
        config
    }

    /// The route and the two limits of an area entry or a path entry at `key`;
    /// a path entry also needs its route (`route_required`). A route of
    /// `auto-merge-eligible` needs both limits.
    fn route_entry(
        &mut self,
        key: &str,
        entry: &Map<String, Value>,
        route_required: bool,
    ) -> AreaRoute {
        let route_key = format!("{key}.route");
        let route = self.route_value(&route_key, entry.get("route"));
        if route_required && !entry.contains_key("route") {
            self.route_fault(&route_key);
        }
        let max_diff_lines = self.count(&format!("{key}.maxDiffLines"), entry.get("maxDiffLines"));
        let max_uncovered_share = self.unit(
            &format!("{key}.maxUncoveredShare"),
            entry.get("maxUncoveredShare"),
        );
        if route == Some(Route::AutoMergeEligible) {
            for field in ["maxDiffLines", "maxUncoveredShare"] {
                if !entry.contains_key(field) {
                    self.fault(
                        &format!("{key}.{field}"),
                        "is needed when the route is \"auto-merge-eligible\"",
                    );
                }
            }
        }
        AreaRoute {
            route,
            max_diff_lines,
            max_uncovered_share,
        }
    }

    /// The glob of a path entry at `key`.
    fn path_glob(&mut self, key: &str, value: Option<&Value>) -> Option<String> {
        let Some(glob) = value.and_then(Value::as_str) else {
            self.fault(&format!("{key}.glob"), "must be a string");
            return None;
        };
        self.glob(&format!("{key}.glob"), glob);
        Some(glob.to_string())
    }

    /// The `review.bundle` settings.
    fn bundle(&mut self, bundle: &Map<String, Value>) -> BundleConfig {
        let mut config = BundleConfig::default();
        if let Some(budgets) = self.section("review.bundle.budgets", bundle.get("budgets")) {
            for (tier, value) in budgets {
                let key = format!("review.bundle.budgets.{tier}");
                if let Some(lines) = self.count(&key, Some(value)) {
                    config.budgets.insert(tier.clone(), lines);
                }
            }
        }
        config.exempt_authors =
            self.strings("review.bundle.exemptAuthors", bundle.get("exemptAuthors"));
        config.exempt_branches =
            self.globs("review.bundle.exemptBranches", bundle.get("exemptBranches"));
        config
    }

    /// The `review.intent` settings. Each source must be an adapter, and each
    /// glob must compile; the config keeps the adapters, each once, and the
    /// globs.
    fn intent(&mut self, intent: &Map<String, Value>) -> IntentConfig {
        let mut config = IntentConfig::default();
        if let Some(sources) = intent.get("sources") {
            config.sources.clear();
            let sources = self.strings("review.intent.sources", Some(sources));
            for (index, source) in sources.iter().enumerate() {
                match Adapter::named(source) {
                    Some(adapter) if !config.sources.contains(&adapter) => {
                        config.sources.push(adapter);
                    }
                    Some(_) => {}
                    None => self.fault(
                        &format!("review.intent.sources[{index}]"),
                        &no_intent_source(source),
                    ),
                }
            }
        }
        config.files = self.compiled_globs("review.intent.files", intent.get("files"));
        config
    }

    /// The `review.sessions` settings. `trailers` must be an array of trailer keys
    /// (letters, digits, and `-`); any other value is one line, whatever the
    /// number of its faults. A key that the list repeats in another case reads
    /// once, as its first spelling.
    fn sessions(&mut self, sessions: &Map<String, Value>) -> SessionsConfig {
        let mut config = SessionsConfig::default();
        if let Some(value) = sessions.get("trailers") {
            match trailer_keys(value) {
                Some(keys) => config.trailers = keys,
                None => self.fault(
                    "review.sessions.trailers",
                    "must be an array of trailer keys (letters, digits, and -)",
                ),
            }
        }
        config
    }

    /// `review.record.maxBytes`: an integer of at least [`MIN_MAX_BYTES`].
    fn max_bytes(&mut self, value: Option<&Value>) -> u64 {
        match value {
            None => DEFAULT_MAX_BYTES,
            Some(value) => match value.as_u64() {
                Some(bytes) if bytes >= MIN_MAX_BYTES => bytes,
                _ => {
                    self.fault(
                        "review.record.maxBytes",
                        &format!("must be an integer of at least {MIN_MAX_BYTES}"),
                    );
                    DEFAULT_MAX_BYTES
                }
            },
        }
    }

    /// The route under `key`, `None` while the key is not set or its value is
    /// no route (a line then names the fault).
    fn route_value(&mut self, key: &str, value: Option<&Value>) -> Option<Route> {
        let value = value?;
        let route = value
            .as_str()
            .and_then(|name| ROUTES.into_iter().find(|route| route.as_str() == name));
        if route.is_none() {
            self.route_fault(key);
        }
        route
    }

    /// The route under `key` for `review.route.default` and `review.route.warn`:
    /// `owner-review` or `senior-review`. `None` while the key is not set, and
    /// when its value is none of the two (a line then names the fault). A value
    /// of `auto-merge-eligible` gets the line that tells how a change reaches
    /// that route (design.md 257).
    fn fallback_route(&mut self, key: &str, value: Option<&Value>) -> Option<Route> {
        let value = value?;
        let route = value
            .as_str()
            .and_then(|name| ROUTES.into_iter().find(|route| route.as_str() == name));
        let allowed = format!(
            "must be \"{}\" or \"{}\"",
            Route::OwnerReview.as_str(),
            Route::SeniorReview.as_str()
        );
        match route {
            Some(route @ (Route::OwnerReview | Route::SeniorReview)) => return Some(route),
            Some(Route::AutoMergeEligible) => self.fault(
                key,
                &format!(
                    "{allowed}: a change is {} only through an area or a path entry that names both limits",
                    Route::AutoMergeEligible.as_str()
                ),
            ),
            None => self.fault(key, &allowed),
        }
        None
    }

    /// The line for a route at `key` that is none of the three.
    fn route_fault(&mut self, key: &str) {
        self.fault(
            key,
            &format!(
                "must be \"{}\", \"{}\", or \"{}\"",
                ROUTES[0].as_str(),
                ROUTES[1].as_str(),
                ROUTES[2].as_str()
            ),
        );
    }

    /// The count under `key`, `None` while the key is not set or its value is
    /// no non-negative integer (a line then names the fault).
    fn count(&mut self, key: &str, value: Option<&Value>) -> Option<u64> {
        let value = value?;
        let count = value.as_u64();
        if count.is_none() {
            self.fault(key, "must be a non-negative integer");
        }
        count
    }

    /// The share, rate, or score from 0 to 1 under `key`, `None` while the key
    /// is not set or its value is none (a line then names the fault).
    fn unit(&mut self, key: &str, value: Option<&Value>) -> Option<f64> {
        let value = value?;
        let number = value.as_f64().filter(|number| (0.0..=1.0).contains(number));
        if number.is_none() {
            self.fault(key, "must be a number from 0 to 1");
        }
        number
    }

    /// The strings under `key`: empty while the key is not set, and also when
    /// its value is no array of strings (a line then names the fault).
    fn strings(&mut self, key: &str, value: Option<&Value>) -> Vec<String> {
        let Some(value) = value else {
            return Vec::new();
        };
        let strings: Option<Vec<String>> = value.as_array().and_then(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect()
        });
        strings.unwrap_or_else(|| {
            self.fault(key, "must be an array of strings");
            Vec::new()
        })
    }

    /// The globs under `key`, like [`Reader::strings`], and a line for each
    /// glob that does not compile.
    fn globs(&mut self, key: &str, value: Option<&Value>) -> Vec<String> {
        let globs = self.strings(key, value);
        for (index, glob) in globs.iter().enumerate() {
            self.glob(&format!("{key}[{index}]"), glob);
        }
        globs
    }

    /// The globs under `key` that compile, like [`Reader::globs`]: a glob that
    /// does not compile has its line and is not in the result.
    fn compiled_globs(&mut self, key: &str, value: Option<&Value>) -> Vec<Glob> {
        let globs = self.strings(key, value);
        globs
            .iter()
            .enumerate()
            .filter_map(|(index, glob)| self.glob(&format!("{key}[{index}]"), glob))
            .map(|matcher| matcher.glob().clone())
            .collect()
    }

    /// The matcher of `glob`, or `None` and the line `<file>: <key>: <the
    /// error of the glob>` while `glob` does not compile.
    fn glob(&mut self, key: &str, glob: &str) -> Option<GlobMatcher> {
        match compile(glob) {
            Ok(matcher) => Some(matcher),
            Err(error) => {
                self.faults.push(format!("{}: {key}: {error}", self.file));
                None
            }
        }
    }
}

/// The trailer keys of `value`, each once, or `None` when `value` is no array of
/// keys. A key is a non-empty string of ASCII letters, digits, and `-`, the
/// characters that git reads in a trailer key; the list keeps the first spelling
/// of a key that it repeats in another case.
fn trailer_keys(value: &Value) -> Option<Vec<String>> {
    let mut keys: Vec<String> = Vec::new();
    for item in value.as_array()? {
        let key = item.as_str()?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return None;
        }
        if !keys.iter().any(|known| known.eq_ignore_ascii_case(key)) {
            keys.push(key.to_string());
        }
    }
    Some(keys)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use indexmap::IndexMap;
    use serde_json::{Value, json};

    use super::*;
    use crate::install::marker::Marker;
    use crate::published::route::Route;

    /// A marker that holds `value`, named as a gate names it.
    fn marker(value: Value) -> Marker {
        Marker::from_json(
            PathBuf::from(".houserules.json"),
            ".houserules.json".to_string(),
            value,
        )
        .expect("a marker")
    }

    /// The example of spec 6.1.
    fn spec_example() -> Value {
        json!({
            "diffExclude": ["Cargo.lock"],
            "route": {
                "default": "owner-review",
                "warn": "owner-review",
                "areas": {"docs": {
                    "route": "auto-merge-eligible", "maxDiffLines": 200, "maxUncoveredShare": 0
                }},
                "paths": [{"glob": "crates/houserules/src/install/**", "route": "senior-review"}],
                "ownerScore": 0.5,
                "seniorScore": 0.8,
                "sampleRate": 0.1,
            },
            "bundle": {
                "budgets": {"light": 400, "standard": 1500},
                "exemptAuthors": ["dependabot[bot]"],
                "exemptBranches": ["release-please--*"],
            },
            "intent": {"sources": ["work-item", "file"], "files": ["docs/plans/*.md"]},
            "ci": {"files": [".github/workflows/ci.yml"]},
            "record": {"maxBytes": 65536},
        })
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    /// Case 9: a marker with no `review` key reads the defaults of spec 6.1:
    /// the `work-item` intent source, a record budget of 65,536 bytes, the
    /// route `owner-review` for a default and for a warned row, and no list.
    #[test]
    fn a_marker_with_no_review_key_reads_the_defaults() {
        let config = marker(json!({"idPrefix": "WI"}))
            .review()
            .expect("defaults");

        assert_eq!(config.intent.adapters(), [Adapter::WorkItem]);
        assert_eq!(config.max_bytes, 65_536);
        assert_eq!(config.route.default, Route::OwnerReview);
        assert_eq!(config.route.warn, Route::OwnerReview);
        assert!(config.diff_exclude.is_empty());
        assert!(config.intent.file_matchers().is_empty());
        assert!(config.route.areas.is_empty());
        assert!(config.route.paths.is_empty());
        assert_eq!(config.route.sample_rate, None);
        assert_eq!(config, ReviewConfig::default());
    }

    /// The example of spec 6.1 reads into every typed field.
    #[test]
    fn the_example_of_the_spec_reads_into_typed_fields() {
        let config = marker(json!({"review": spec_example()}))
            .review()
            .expect("the example is sound");

        assert_eq!(config.diff_exclude, strings(&["Cargo.lock"]));
        assert_eq!(config.route.default, Route::OwnerReview);
        assert_eq!(config.route.warn, Route::OwnerReview);
        assert_eq!(
            config.route.areas,
            IndexMap::from([(
                "docs".to_string(),
                AreaRoute {
                    route: Some(Route::AutoMergeEligible),
                    max_diff_lines: Some(200),
                    max_uncovered_share: Some(0.0),
                }
            )])
        );
        assert_eq!(
            config.route.paths,
            [PathRoute {
                glob: "crates/houserules/src/install/**".to_string(),
                route: Route::SeniorReview,
                max_diff_lines: None,
                max_uncovered_share: None,
            }]
        );
        assert_eq!(config.route.owner_score, Some(0.5));
        assert_eq!(config.route.senior_score, Some(0.8));
        assert_eq!(config.route.sample_rate, Some(0.1));
        assert_eq!(
            config.bundle,
            BundleConfig {
                budgets: IndexMap::from([
                    ("light".to_string(), 400),
                    ("standard".to_string(), 1500)
                ]),
                exempt_authors: strings(&["dependabot[bot]"]),
                exempt_branches: strings(&["release-please--*"]),
            }
        );
        assert_eq!(config.intent.adapters(), [Adapter::WorkItem, Adapter::File]);
        let matchers = config.intent.file_matchers();
        assert_eq!(matchers.len(), 1);
        assert!(matchers[0].is_match("docs/plans/a.md"));
        assert_eq!(config.max_bytes, 65_536);
    }

    /// A key that is set replaces its default, and a section that sets other
    /// keys keeps the defaults of the rest. An empty `sources` list is the
    /// project's choice and stays empty.
    #[test]
    fn a_key_that_is_set_replaces_its_default() {
        let config = marker(json!({"review": {
            "route": {"warn": "senior-review"},
            "intent": {"files": ["docs/plans/*.md"]},
            "record": {},
        }}))
        .review()
        .expect("sound");
        assert_eq!(config.route.warn, Route::SeniorReview);
        assert_eq!(config.route.default, Route::OwnerReview);
        assert_eq!(config.intent.adapters(), [Adapter::WorkItem]);
        assert_eq!(config.max_bytes, 65_536);

        let config =
            marker(json!({"review": {"intent": {"sources": []}, "record": {"maxBytes": 8192}}}))
                .review()
                .expect("sound");
        assert!(config.intent.adapters().is_empty());
        assert_eq!(config.max_bytes, 8192);
    }

    /// The faults come one line each, in the order of the sections of spec
    /// 6.1, not of the file, and each line names the marker and the key.
    #[test]
    fn the_faults_come_in_the_order_of_the_sections() {
        let errors = marker(json!({"review": {
            "record": {"maxBytes": 100},
            "ci": {"files": "x"},
            "intent": {"sources": ["nope"]},
            "bundle": {"budgets": {"light": -1}},
            "route": {"default": "never", "sampleRate": 2},
            "diffExclude": 3,
        }}))
        .review()
        .expect_err("faults");

        assert_eq!(
            errors,
            [
                ".houserules.json: review.diffExclude must be an array of strings",
                ".houserules.json: review.route.default must be \"owner-review\" or \"senior-review\"",
                ".houserules.json: review.route.sampleRate must be a number from 0 to 1",
                ".houserules.json: review.bundle.budgets.light must be a non-negative integer",
                ".houserules.json: review.intent.sources[0] \"nope\" is no intent source; use one of: work-item, file, openspec",
                ".houserules.json: review.ci.files must be an array of strings",
                ".houserules.json: review.record.maxBytes must be an integer of at least 8192",
            ]
        );
    }

    /// `review.ci.files` is checked and not stored: a list of strings changes
    /// nothing in the config, and an entry that is no string has its line. The
    /// change analysis reads the list from the marker at each commit.
    #[test]
    fn the_ci_files_are_checked_and_not_stored() {
        let config = marker(json!({"review": {"ci": {"files": [".github/workflows/ci.yml"]}}}))
            .review()
            .expect("sound");
        assert_eq!(config, ReviewConfig::default());

        let errors = marker(json!({"review": {"ci": {"files": ["ci.yml", 1]}}}))
            .review()
            .expect_err("fault");
        assert_eq!(
            errors,
            [".houserules.json: review.ci.files must be an array of strings"]
        );
    }

    /// A score threshold lies between 0 and 1, like a share and a rate (spec
    /// 6.1; the range of `--score`, spec 5.1): the ends pass.
    #[test]
    fn a_score_threshold_of_0_or_1_passes() {
        let config = marker(json!({"review": {"route": {"ownerScore": 0, "seniorScore": 1.0}}}))
            .review()
            .expect("sound");

        assert_eq!(config.route.owner_score, Some(0.0));
        assert_eq!(config.route.senior_score, Some(1.0));
    }

    /// A score threshold above 1, below 0, or that is no number is one line.
    #[test]
    fn a_score_threshold_outside_0_to_1_or_no_number_is_one_line() {
        let errors = marker(json!({"review": {"route": {
            "ownerScore": -0.5,
            "seniorScore": 1.5,
            "sampleRate": 1.5,
        }}}))
        .review()
        .expect_err("faults");

        assert_eq!(
            errors,
            [
                ".houserules.json: review.route.ownerScore must be a number from 0 to 1",
                ".houserules.json: review.route.seniorScore must be a number from 0 to 1",
                ".houserules.json: review.route.sampleRate must be a number from 0 to 1",
            ]
        );
        let errors = marker(json!({"review": {"route": {"seniorScore": "high"}}}))
            .review()
            .expect_err("faults");
        assert_eq!(
            errors,
            [".houserules.json: review.route.seniorScore must be a number from 0 to 1"]
        );
    }

    /// Design.md 257: `default` and `warn` take `owner-review` or
    /// `senior-review`. A value of `auto-merge-eligible` is one line that
    /// names the key and the way to that route, and it asks for no
    /// `sampleRate`: the sample belongs to an area or a path entry.
    #[test]
    fn a_default_or_warn_of_auto_merge_eligible_is_one_line_each() {
        let errors = marker(json!({"review": {"route": {
            "default": "auto-merge-eligible",
            "warn": "auto-merge-eligible",
        }}}))
        .review()
        .expect_err("faults");

        let rest = r#"must be "owner-review" or "senior-review": a change is auto-merge-eligible only through an area or a path entry that names both limits"#;
        assert_eq!(
            errors,
            [
                format!(".houserules.json: review.route.default {rest}"),
                format!(".houserules.json: review.route.warn {rest}"),
            ]
        );
    }

    /// A `default` or a `warn` that is no route at all names the two routes
    /// that it takes.
    #[test]
    fn a_default_or_warn_that_is_no_route_names_the_two_routes() {
        let errors = marker(json!({"review": {"route": {"default": 3, "warn": "never"}}}))
            .review()
            .expect_err("faults");

        let rest = r#"must be "owner-review" or "senior-review""#;
        assert_eq!(
            errors,
            [
                format!(".houserules.json: review.route.default {rest}"),
                format!(".houserules.json: review.route.warn {rest}"),
            ]
        );
    }

    /// `review_in_areas` gives a line for each area that the project does not
    /// define, before the other lines of that area and beside every other
    /// fault of the key. `review` does not know the areas and gives no such
    /// line.
    #[test]
    fn an_undefined_area_has_its_line_before_its_other_lines() {
        let marker = marker(json!({"review": {
            "record": {"maxBytes": 100},
            "route": {
                "areas": {
                    "docs": {"route": "owner"},
                    "nowhere": {"route": "owner"},
                    "also": "x",
                },
                "default": "never",
            },
        }}));
        let route = ".houserules.json: review.route.areas";
        let routes = r#"must be "auto-merge-eligible", "owner-review", or "senior-review""#;
        let fallback = r#"must be "owner-review" or "senior-review""#;

        let errors = marker
            .review_in_areas(&["docs", "tools"])
            .expect_err("faults");

        assert_eq!(
            errors,
            [
                format!(".houserules.json: review.route.default {fallback}"),
                format!("{route}.docs.route {routes}"),
                format!("{route}.nowhere is not an area of knowledge/areas.json"),
                format!("{route}.nowhere.route {routes}"),
                format!("{route}.also is not an area of knowledge/areas.json"),
                format!("{route}.also must be an object"),
                ".houserules.json: review.record.maxBytes must be an integer of at least 8192"
                    .to_string(),
            ]
        );
        let unchecked = marker.review().expect_err("faults");
        let without_area_lines: Vec<String> = errors
            .into_iter()
            .filter(|line| !line.contains("is not an area of"))
            .collect();
        assert_eq!(unchecked, without_area_lines);
    }

    /// An area that the project defines gets no line, and the config that
    /// `review_in_areas` gives equals the one that `review` gives.
    #[test]
    fn a_defined_area_has_no_line() {
        let marker = marker(json!({"review": spec_example()}));

        let config = marker.review_in_areas(&["docs"]).expect("docs is defined");

        assert_eq!(config, marker.review().expect("sound"));
        assert!(marker.review_in_areas(&["tools"]).is_err());
    }

    /// The intent sources that the lint accepts are the three adapters, in the
    /// order of the config example and then of the `openspec` adapter (spec
    /// 3.6).
    #[test]
    fn the_adapters_are_work_item_file_and_openspec() {
        assert_eq!(ADAPTERS, ["work-item", "file", "openspec"]);
    }

    /// `openspec` is a source that the lint accepts, once, at the place of its
    /// first mention.
    #[test]
    fn openspec_is_an_adapter_that_the_lint_accepts() {
        let config = marker(json!({"review": {"intent": {
            "sources": ["openspec", "file", "openspec"],
        }}}))
        .review()
        .expect("sound");

        assert_eq!(config.intent.adapters(), [Adapter::OpenSpec, Adapter::File]);
    }

    /// A source that the list names twice is one adapter, at the place of its
    /// first mention.
    #[test]
    fn a_source_that_is_listed_twice_is_one_adapter() {
        let config = marker(json!({"review": {"intent": {
            "sources": ["file", "work-item", "file"],
        }}}))
        .review()
        .expect("sound");

        assert_eq!(config.intent.adapters(), [Adapter::File, Adapter::WorkItem]);
    }

    /// The globs of `review.intent.files` match as every other glob of the
    /// kit does: `*` stops at a folder separator, and `**` crosses it.
    #[test]
    fn the_globs_of_the_intent_files_match_like_every_other_glob() {
        let config = marker(json!({"review": {"intent": {
            "files": ["docs/plans/*.md", "specs/**/*.md"],
        }}}))
        .review()
        .expect("sound");

        let matches = |path: &str| {
            config
                .intent
                .file_matchers()
                .iter()
                .any(|matcher| matcher.is_match(path))
        };
        assert!(matches("docs/plans/a.md"));
        assert!(!matches("docs/plans/old/a.md"));
        assert!(matches("specs/a/b.md"));
        assert!(!matches("docs/other.md"));
    }

    /// The first line of a file that `houserules ci write` created is the
    /// text of spec 10.3, byte for byte.
    #[test]
    fn the_first_line_of_a_written_ci_file_is_the_text_of_the_spec() {
        assert_eq!(
            CI_WRITTEN_BY_LINE,
            "# Written by houserules ci write; rewrite it with houserules ci write --force and the flags that wrote it."
        );
    }

    /// `add_ci_file` creates `review.ci.files` where the marker has none, goes
    /// last among the keys of the marker, appends in call order, and reports
    /// `false` for a path that the list holds.
    #[test]
    fn add_ci_file_appends_a_path_once() {
        let mut marker = marker(json!({"idPrefix": "HR", "version": "2.0.0"}));

        assert!(marker.add_ci_file(".github/workflows/houserules-review.yml"));
        assert!(marker.add_ci_file(".gitlab-ci.yml"));
        assert!(!marker.add_ci_file(".github/workflows/houserules-review.yml"));

        let keys: Vec<&String> = marker.map.keys().collect();
        assert_eq!(keys, ["idPrefix", "version", "review"]);
        assert_eq!(
            marker.map["review"],
            json!({"ci": {"files": [".github/workflows/houserules-review.yml", ".gitlab-ci.yml"]}})
        );
        assert!(marker.review().is_ok(), "the list reads without a fault");
    }

    /// `add_ci_file` keeps every other key of `review` and the order of the
    /// list.
    #[test]
    fn add_ci_file_keeps_the_other_keys_of_review() {
        let mut marker = marker(json!({"review": {
            "ci": {"files": [".github/workflows/ci.yml"], "other": true},
            "record": {"maxBytes": 9000},
        }}));

        assert!(marker.add_ci_file("ci/pipeline.yml"));

        assert_eq!(
            marker.map["review"],
            json!({
                "ci": {"files": [".github/workflows/ci.yml", "ci/pipeline.yml"], "other": true},
                "record": {"maxBytes": 9000},
            })
        );
    }

    /// `add_ci_file` adds nothing where the key has the wrong shape, reports
    /// `false`, and leaves the marker as found: `check-knowledge` names the
    /// fault.
    #[test]
    fn add_ci_file_leaves_a_value_of_the_wrong_shape_alone() {
        for review in [json!([]), json!({"ci": "x"}), json!({"ci": {"files": "x"}})] {
            let mut marker = marker(json!({"review": review.clone()}));
            assert!(!marker.add_ci_file("ci.yml"), "{review}");
            assert_eq!(marker.map["review"], review);
        }
    }

    /// `reviewLayerOffered` is `false` until `set_review_layer_offered` sets it;
    /// the key goes last when new and keeps its place when it exists.
    #[test]
    fn the_offer_of_the_review_layer_is_recorded_once() {
        let mut marker = marker(json!({"idPrefix": "HR", "version": "2.0.0"}));
        assert!(!marker.review_layer_offered());

        marker.set_review_layer_offered();

        assert!(marker.review_layer_offered());
        let keys: Vec<&String> = marker.map.keys().collect();
        assert_eq!(keys, ["idPrefix", "version", "reviewLayerOffered"]);

        let mut placed = self::marker(json!({"reviewLayerOffered": false, "version": "2.0.0"}));
        assert!(!placed.review_layer_offered());
        placed.set_review_layer_offered();
        let keys: Vec<&String> = placed.map.keys().collect();
        assert_eq!(keys, ["reviewLayerOffered", "version"]);
        assert_eq!(placed.map["reviewLayerOffered"], json!(true));
    }

    /// Batch 38, spec 5.7: `review.sessions.trailers` lists the keys of the trailers
    /// that name a session. Without the key the list is `Entire-Checkpoint`; `[]` is
    /// the project's choice of no key; a key that the list repeats in another case
    /// reads once, as its first spelling.
    #[test]
    fn the_session_trailers_default_to_entire_checkpoint_and_the_list_replaces_them() {
        let read = |sessions: Value| {
            marker(json!({"review": {"sessions": sessions}}))
                .review()
                .expect("sound")
                .sessions
                .trailers
        };
        let default = strings(&["Entire-Checkpoint"]);
        assert_eq!(
            marker(json!({"idPrefix": "WI"}))
                .review()
                .expect("defaults")
                .sessions
                .trailers,
            default
        );
        assert_eq!(
            marker(json!({"review": {}}))
                .review()
                .expect("sound")
                .sessions
                .trailers,
            default
        );
        assert_eq!(read(json!({})), default);
        assert_eq!(read(json!({"trailers": []})), Vec::<String>::new());
        assert_eq!(
            read(json!({"trailers": ["Session-Id", "Co-Thread"]})),
            strings(&["Session-Id", "Co-Thread"])
        );
        assert_eq!(
            read(json!({"trailers": ["Session-Id", "session-id", "SESSION-ID", "X-1"]})),
            strings(&["Session-Id", "X-1"])
        );
    }

    /// A list of session trailers with a fault is one line, whatever the number of
    /// its faults, and a `sessions` value that is no object is one line of its own.
    #[test]
    fn a_session_trailer_list_with_a_fault_is_one_line() {
        let line = ".houserules.json: review.sessions.trailers must be an array of trailer keys (letters, digits, and -)".to_string();
        for trailers in [
            json!("Session-Id"),
            json!(5),
            json!(null),
            json!({"a": 1}),
            json!(["Session-Id", 1]),
            json!([""]),
            json!(["a b"]),
            json!(["a:b"]),
            json!(["Session_Id"]),
            json!(["caf\u{e9}"]),
            json!(["a b", "c:d"]),
        ] {
            assert_eq!(
                marker(json!({"review": {"sessions": {"trailers": trailers}}}))
                    .review()
                    .err(),
                Some(vec![line.clone()]),
                "{trailers}"
            );
        }
        for sessions in [json!([]), json!("trailers"), json!(null)] {
            assert_eq!(
                marker(json!({"review": {"sessions": sessions}}))
                    .review()
                    .err(),
                Some(vec![
                    ".houserules.json: review.sessions must be an object".to_string()
                ]),
                "{sessions}"
            );
        }
    }
}

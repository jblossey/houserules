//! The assertions that every wiring of the review layer must pass (spec 10.1,
//! 10.2, and the unit parts of acceptance case 8). The two published units
//! (`.github/workflows/houserules-review.yml` and `ci/gitlab-review.yml`) pass
//! them in `ci_units.rs`. A test of an inline form that `houserules ci` prints
//! or writes calls the same assertions.
//!
//! [`parse_unit`] reads a unit with `saphyr` into a [`Unit`]: the jobs by role
//! (`gates`, `floor`, `report`, `record`, `record-push`), the scripts of each
//! job, and the variables of each job. Each assertion states one trust rule of
//! spec 10.1 and panics with the rule that failed. A test file that needs this
//! module declares it, as `tracker_support/mod.rs` is shared. The module
//! allows dead code and unused imports, because the test files that include it
//! use different parts of it. A file that only reads workflows or runs the
//! sandbox uses none of the re-exported assertions. The allowance sits here, so
//! that no includer repeats it.

#![allow(dead_code, unused_imports)]

#[cfg(unix)]
pub mod sandbox;

mod assertions;
/// The key function of the command, which the tests call as the command does.
#[path = "../../src/review/ci/key.rs"]
mod key;
mod shell;
mod unit;

pub use assertions::*;
pub(crate) use key::install_key;
pub use shell::{bare_words, invokes, traces_the_shell};
pub use unit::{Host, Unit, parse_inline_unit, parse_unit};

/// The five jobs of every wiring, in the order of spec 10.1.
pub const ROLES: [&str; 5] = ["gates", "floor", "report", "record", "record-push"];

/// The two jobs that hold a write credential. No other job runs houserules, the
/// setup command, or code of the project with one.
pub const WRITE_ROLES: [&str; 2] = ["report", "record-push"];

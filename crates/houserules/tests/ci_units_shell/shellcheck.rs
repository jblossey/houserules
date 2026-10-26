//! Every shell text of both units passes shellcheck.

use super::*;

/// Every shell text of both units passes shellcheck: the scripts of the jobs in
/// the dialect of their shell (`bash` on GitHub, `sh` on GitLab) and the
/// shared functions in POSIX `sh`.
#[test]
fn every_script_of_both_units_passes_shellcheck() {
    let github = github();
    let gitlab = gitlab("root");
    let mut texts: Vec<(String, &str, String)> = Vec::new();
    for role in ROLES {
        for script in github.scripts(role) {
            texts.push((
                format!("github {role} {}", script.name),
                "bash",
                script.text,
            ));
        }
        for script in gitlab.scripts(role) {
            texts.push((format!("gitlab {role}"), "sh", script.text));
        }
    }
    for (host, unit) in [("github", &github), ("gitlab", &gitlab)] {
        for (role, name) in [
            ("gates", "HOUSERULES_SH"),
            ("report", "HOUSERULES_ROUTE_SH"),
        ] {
            let shared = unit.variable(role, name).expect("a shared shell text");
            texts.push((format!("{host} {name}"), "sh", shared));
        }
    }
    let directory = tempfile::tempdir().expect("create a scratch directory");
    for (index, (label, dialect, script)) in texts.iter().enumerate() {
        let file = directory.path().join(format!("script-{index}.sh"));
        fs::write(&file, script).expect("write a script");
        let output = Command::new("shellcheck")
            .args(["-s", dialect])
            .arg(&file)
            .output()
            .unwrap_or_else(|error| {
                panic!(
                    "shellcheck is not installed ({error}): run mise install, which pins it in mise.toml"
                )
            });
        assert!(
            output.status.success(),
            "shellcheck rejects {label}:\n{}",
            stdout(&output)
        );
    }
    assert!(
        texts.len() > 20,
        "the units hold their scripts: {}",
        texts.len()
    );
}

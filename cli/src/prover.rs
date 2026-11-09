//! Where this `names` runs programs and makes proofs, settled once as it starts.
//!
//! Before a call is sent, its programs run here (a public call is tried first; a private one
//! is proven). RISC Zero does both either inside this process (a build with the `local-prove`
//! feature) or in its `r0vm`. Left to itself, risc0 looks for an `r0vm` only when the first
//! program runs, falls back from a `RISC0_SERVER_PATH` that names no file to whatever else it
//! finds, and then fails with a bare "No such file or directory", or panics when
//! `RISC0_PROVER=local` asks a build that can't prove in-process. So it is decided here
//! instead, once: the `r0vm` found is handed to risc0 as `RISC0_SERVER_PATH`, which it then
//! uses and nothing else, and every call that would run a program asks first and is refused
//! in plain words, before anything is sent, when there is nothing to run it with.

use std::path::PathBuf;
use std::sync::OnceLock;

/// What runs a program or makes a proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Engine {
    /// Inside this process (`local-prove`).
    InProcess,
    /// The `r0vm` at this path.
    External(PathBuf),
}

/// Where an `r0vm` can be, in the order it is looked for.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// `RISC0_SERVER_PATH`, if set.
    pub server_path: Option<PathBuf>,
    /// Beside the `names` executable, where the installer puts the one it ships.
    pub beside: Option<PathBuf>,
    /// The copies rzup installed for this risc0 version (`~/.risc0`).
    pub installed: Vec<PathBuf>,
    /// The first `r0vm` on `PATH`.
    pub on_path: Option<PathBuf>,
}

impl Places {
    /// The first of them that is a file.
    fn r0vm(&self) -> Option<PathBuf> {
        let is_file = |path: &&PathBuf| path.is_file();
        self.server_path
            .iter()
            .chain(&self.beside)
            .chain(&self.installed)
            .chain(&self.on_path)
            .find(is_file)
            .cloned()
    }

    /// Where it was looked for, in words.
    fn searched(&self) -> String {
        let mut places = Vec::new();
        if let Some(path) = &self.server_path {
            places.push(format!("in RISC0_SERVER_PATH ({})", path.display()));
        }
        match self.beside.as_ref().and_then(|path| path.parent()) {
            Some(dir) => places.push(format!("beside names in {}", dir.display())),
            None => places.push("beside names".to_owned()),
        }
        places.push("in ~/.risc0".to_owned());
        places.push("on PATH".to_owned());
        let last = places.pop().unwrap_or_default();
        if places.is_empty() { last } else { format!("{} or {last}", places.join(", ")) }
    }
}

/// What runs programs (`job` "executor", read from `RISC0_EXECUTOR`) or makes proofs
/// (`job` "prover", from `RISC0_PROVER`), as risc0 will choose it given `asked` and whether
/// this build proves in-process; `Err` says in words why there is nothing to do it with.
pub fn settle(job: &str, asked: Option<&str>, in_process_built: bool, places: &Places) -> Result<Engine, String> {
    let variable = if job == "prover" { "RISC0_PROVER" } else { "RISC0_EXECUTOR" };
    let asked = asked.map(|value| value.trim().to_ascii_lowercase()).filter(|value| !value.is_empty());
    let external = || {
        places.r0vm().map(Engine::External).ok_or_else(|| {
            format!(
                "no r0vm was found {}. Reinstall names with the r0vm that comes with it, or set RISC0_SERVER_PATH to an r0vm, then try again",
                places.searched()
            )
        })
    };
    match asked.as_deref() {
        None if in_process_built => Ok(Engine::InProcess),
        None | Some("ipc" | "actor") => external(),
        Some("local") if in_process_built => Ok(Engine::InProcess),
        Some("local") => Err(format!(
            "{variable}=local asks for it inside names, and this names was built without that (the local-prove feature). Unset {variable} to use r0vm, then try again"
        )),
        Some(other) => Err(format!("{variable}={other} isn't one names can use. Unset it, or set it to local or ipc, then try again")),
    }
}

/// What was settled as `names` started (`start`).
struct Settled {
    executor: Result<Engine, String>,
    prover: Result<Engine, String>,
}

static SETTLED: OnceLock<Settled> = OnceLock::new();

/// The `r0vm` files rzup keeps for this risc0 version, as risc0 itself would find them.
fn installed() -> Vec<PathBuf> {
    // rzup makes its folder when it starts: with none there, there is nothing it installed.
    let home = std::env::var_os("RISC0_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".risc0")));
    if !home.is_some_and(|home| home.is_dir()) {
        return Vec::new();
    }
    let Ok(version) = risc0_zkvm::get_version() else {
        return Vec::new();
    };
    let Ok(rzup) = rzup::Rzup::new() else {
        return Vec::new();
    };
    // As risc0 looks: this version, then the same with the patch level dropped.
    let loose = format!("{}.{}.0", version.major, version.minor).parse().ok();
    [Some(version), loose]
        .into_iter()
        .flatten()
        .filter_map(|version| rzup.get_version_dir(&rzup::Component::R0Vm, &version).ok())
        .map(|dir| dir.join("r0vm"))
        .collect()
}

fn on_path() -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join("r0vm")).find(|path| path.is_file())
}

fn env_text(name: &str) -> Option<String> {
    std::env::var_os(name).map(|value| value.to_string_lossy().into_owned())
}

/// Settles both, and hands the `r0vm` found to risc0. Called first thing in `main`, before any
/// other thread starts (it sets an environment variable).
pub fn start() {
    let places = Places {
        server_path: std::env::var_os("RISC0_SERVER_PATH").map(PathBuf::from),
        beside: std::env::current_exe()
            .ok()
            .and_then(|exe| exe.canonicalize().ok())
            .and_then(|exe| Some(exe.parent()?.join("r0vm"))),
        installed: installed(),
        on_path: on_path(),
    };
    let built = cfg!(feature = "local-prove");
    let settled = Settled {
        executor: settle("executor", env_text("RISC0_EXECUTOR").as_deref(), built, &places),
        prover: settle("prover", env_text("RISC0_PROVER").as_deref(), built, &places),
    };
    // risc0 is handed the r0vm found, so it uses that one. When none was found here, risc0's
    // own search (the same places) finds none either; every call that would run a program is
    // refused before that, by `to_run` or `to_prove`.
    let r0vm = [&settled.executor, &settled.prover].into_iter().find_map(|engine| match engine {
        Ok(Engine::External(path)) => Some(path.clone()),
        _ => None,
    });
    if let Some(path) = r0vm {
        // SAFETY: called first thing in main, before any other thread starts.
        unsafe { std::env::set_var("RISC0_SERVER_PATH", path) };
    }
    let _ = SETTLED.set(settled);
}

fn refusal(what: &str, reason: &str) -> anyhow::Error {
    anyhow::anyhow!("names can't {what} on this computer: {reason}. Nothing was sent.")
}

/// Before a call's programs are tried here (a public call, before it is sent).
pub fn to_run() -> anyhow::Result<()> {
    match SETTLED.get().map(|settled| &settled.executor) {
        Some(Err(reason)) => Err(refusal("run the program this call needs", reason)),
        _ => Ok(()),
    }
}

/// Before the first proof of a private change, and before any step of a job that has one.
pub fn to_prove() -> anyhow::Result<()> {
    match SETTLED.get().map(|settled| &settled.prover) {
        Some(Err(reason)) => Err(refusal("make the proof a private change needs", reason)),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("names-prover-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn a_plain_build_with_no_r0vm_anywhere_is_refused_in_words() {
        let empty = dir("none");
        let places = Places {
            server_path: None,
            beside: Some(empty.join("r0vm")),
            installed: vec![empty.join("v3.0.5/r0vm")],
            on_path: None,
        };
        let reason = settle("prover", None, false, &places).unwrap_err();
        assert_eq!(
            reason,
            format!(
                "no r0vm was found beside names in {}, in ~/.risc0 or on PATH. Reinstall names with the r0vm that comes with it, or set RISC0_SERVER_PATH to an r0vm, then try again",
                empty.display()
            )
        );
        assert!(settle("executor", None, false, &places).is_err());
        let said = refusal("make the proof a private change needs", &reason).to_string();
        assert!(said.starts_with("names can't make the proof a private change needs on this computer: no r0vm was found"), "{said}");
        assert!(said.ends_with("then try again. Nothing was sent."), "{said}");
    }

    #[test]
    fn a_server_path_that_names_no_file_is_not_a_prover_and_nothing_else_is_guessed() {
        // The 2026-11-09 slip: RISC0_SERVER_PATH=/nonexistent/r0vm was believed to leave no
        // prover, and risc0 fell back to ~/.risc0. Here it is looked past only to the places
        // listed, and when none holds a file there is no prover.
        let empty = dir("server");
        let places = Places { server_path: Some(PathBuf::from("/nonexistent/r0vm")), beside: Some(empty.join("r0vm")), ..Places::default() };
        let reason = settle("prover", Some("ipc"), false, &places).unwrap_err();
        assert!(reason.starts_with("no r0vm was found in RISC0_SERVER_PATH (/nonexistent/r0vm), beside names in "), "{reason}");
    }

    #[test]
    fn the_first_r0vm_found_is_the_one_used() {
        let place = dir("found");
        let beside = file(&place, "r0vm");
        let installed = file(&place, "installed-r0vm");
        let places = Places { server_path: Some(place.join("missing")), beside: Some(beside.clone()), installed: vec![installed.clone()], on_path: None };
        assert_eq!(settle("prover", None, false, &places), Ok(Engine::External(beside)));
        let places = Places { beside: Some(place.join("absent")), installed: vec![installed.clone()], ..Places::default() };
        assert_eq!(settle("executor", None, false, &places), Ok(Engine::External(installed)));
    }

    #[test]
    fn a_local_prove_build_proves_in_process_without_any_r0vm() {
        let places = Places::default();
        assert_eq!(settle("prover", None, true, &places), Ok(Engine::InProcess));
        assert_eq!(settle("prover", Some("LOCAL"), true, &places), Ok(Engine::InProcess));
        assert_eq!(settle("executor", Some(""), true, &places), Ok(Engine::InProcess));
        // Asking it for r0vm still needs one.
        assert!(settle("prover", Some("ipc"), true, &places).unwrap_err().starts_with("no r0vm was found"));
    }

    #[test]
    fn asking_a_plain_build_to_prove_in_process_is_refused_not_a_panic() {
        let place = dir("local");
        let places = Places { beside: Some(file(&place, "r0vm")), ..Places::default() };
        let reason = settle("prover", Some("local"), false, &places).unwrap_err();
        assert_eq!(
            reason,
            "RISC0_PROVER=local asks for it inside names, and this names was built without that (the local-prove feature). Unset RISC0_PROVER to use r0vm, then try again"
        );
        // The same build still runs public calls with the r0vm beside it.
        assert!(settle("executor", None, false, &places).is_ok());
        assert_eq!(
            settle("prover", Some("bonsai"), false, &places).unwrap_err(),
            "RISC0_PROVER=bonsai isn't one names can use. Unset it, or set it to local or ipc, then try again"
        );
    }
}

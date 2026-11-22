//! What the language server asks of the driver: the layout a file is
//! formatted in, which design a file is in, and that design loaded as
//! `check` with no arguments analyses it, through the server's buffers.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use astli_fmt::Options;
use astli_lsp::{Analysed, At, Place, Problem, Snapshot, Unit, Watched};
use astli_package::{BENDER_MANIFEST, EXTENSIONS, MANIFEST, mark};
use astli_resolve::LOCKFILE;
use astli_text::Reader;

use crate::analysis::{Analysis, Designs};
use crate::config::FmtConfig;
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::run::Run;
use crate::target::Target;
use crate::term;

/// The driver's side of the server: what each design root's loads keep
/// for the next.
#[derive(Default)]
pub struct Host {
    /// What each design root's loads keep, by the root.
    roots: Mutex<HashMap<PathBuf, Kept>>,
    /// The last ticket handed out, to a load as it starts or to a root as
    /// it is forgotten: a later one is a larger number.
    tickets: AtomicU64,
}

/// What a design root's loads keep for the next, each part with the ticket
/// of the load that kept it. Loads of one root overlap, one an edit
/// overtook finishing as the next runs, and what a later load kept is never
/// replaced by an earlier one's.
///
/// A root forgotten keeps its path and its floor, so that a load started
/// before keeps nothing when it ends.
#[derive(Default)]
struct Kept {
    /// The ticket a load needs at least to keep anything: one handed out
    /// after the root was last forgotten.
    floor: u64,
    /// The plan of the root's last discovery, with the problems it found,
    /// which a load that does not discover again reports too; `None` if it
    /// failed, so that the next load discovers again and tells why.
    plan: (u64, Option<(Plan, Vec<term::Problem>)>),
}

impl astli_lsp::Host for Host {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// The `[fmt]` of the package the file is in, as `astli fmt` run beside
    /// it reads it, or `None` if its `exclude` names the file. The manifest
    /// is read on each request, so that an edit to it counts from the next
    /// format on.
    fn fmt_options(&self, path: &Path) -> std::result::Result<Option<Options>, String> {
        let dir = path.parent().unwrap_or(path);
        let config = FmtConfig::load(&Target::new(dir.to_path_buf()), None);
        let config = config.map_err(|err| err.to_string())?;
        Ok((!config.excludes(path)).then_some(config.options))
    }

    fn design(&self, path: &Path) -> PathBuf {
        Plan::design(path)
    }

    fn load(
        &self,
        root: &Path,
        reader: &dyn Reader,
        discover: bool,
        overtaken: &(dyn Fn() -> bool + Sync),
    ) -> Option<Snapshot> {
        let ticket = self.ticket();
        let run = Run::loading(reader).abandoning(overtaken);
        let mut problems = Vec::new();
        let designs = match self.analysed(root, &run, discover, ticket, &mut problems) {
            Ok(designs) => designs,
            Err(Error::Overtaken) => return None,
            // Its problems are kept already.
            Err(Error::Silent) => Vec::new(),
            Err(err) => {
                err.problem().report(&run);
                Vec::new()
            }
        };
        problems.extend(run.problems());
        Some(Snapshot {
            root: root.to_path_buf(),
            lowered: designs.iter().map(|it| it.units.len()).sum(),
            designs,
            problems: problems.iter().map(plain).collect(),
        })
    }

    /// Drops what `root`'s loads kept on the pool, since the server asks on
    /// its loop.
    fn forget(&self, root: &Path) {
        let floor = self.ticket();
        let mut roots = held(&self.roots);
        let kept = roots.entry(root.to_path_buf()).or_default();
        let forgotten = std::mem::take(kept);
        kept.floor = floor;
        drop(roots);
        rayon::spawn(move || drop(forgotten));
    }

    /// What discovery reads besides the sources: the manifests, the lock,
    /// and a dependency's Bender manifest. A source the walk would find is
    /// one of the extensions it takes.
    fn watched(&self) -> Watched {
        let names = [MANIFEST, LOCKFILE, BENDER_MANIFEST];
        Watched {
            names: names.map(String::from).to_vec(),
            extensions: EXTENSIONS.map(String::from).to_vec(),
        }
    }
}

impl Host {
    fn ticket(&self) -> u64 {
        self.tickets.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Runs `keep` on what `root`'s loads keep, unless the root was
    /// forgotten since `ticket` was handed out. What `keep` hands back, such
    /// as what it replaced, is dropped once the lock is let go.
    fn keep<T>(&self, root: &Path, ticket: u64, keep: impl FnOnce(&mut Kept) -> T) -> Option<T> {
        let mut roots = held(&self.roots);
        let kept = roots.entry(root.to_path_buf()).or_default();
        (ticket >= kept.floor).then(|| keep(kept))
    }

    /// Each design `check` analyses of the design at `root`, adding what
    /// its discovery found to `problems`.
    fn analysed(
        &self,
        root: &Path,
        run: &Run,
        discover: bool,
        ticket: u64,
        problems: &mut Vec<term::Problem>,
    ) -> Result<Vec<Analysed>> {
        let plan = self.plan(root, run, discover, ticket, problems)?;
        let mut designs = Designs::of(&mut io::sink(), &plan, &[], false, &[], run)?;
        let analyse = |_: &mut _, plan, analysis, _: &mut _, _| Ok(analysed(&plan, analysis));
        designs.each_kept(&mut io::sink(), plan, run, true, analyse)
    }

    /// The plan of the design at `root`: the last one discovered, unless
    /// `discover` or there is none, with the problems its discovery found
    /// added to `problems`. One discovered is kept, by the load with
    /// `ticket`, unless a later load's discovery was kept already.
    fn plan(
        &self,
        root: &Path,
        run: &Run,
        discover: bool,
        ticket: u64,
        problems: &mut Vec<term::Problem>,
    ) -> Result<Plan> {
        if !discover {
            let roots = held(&self.roots);
            if let Some((plan, found)) = roots.get(root).and_then(|it| it.plan.1.as_ref()) {
                problems.extend(found.iter().cloned());
                return Ok(plan.clone());
            }
        }
        let plan = Plan::load(root, run);
        let found = plan.as_ref().ok().map(|plan| {
            let found = run.problems();
            problems.extend(found.iter().cloned());
            (plan.clone(), found)
        });
        let dropped = self.keep(root, ticket, |kept| match kept.plan.0 > ticket {
            true => found,
            false => {
                kept.plan.0 = ticket;
                std::mem::replace(&mut kept.plan.1, found)
            }
        });
        drop(dropped);
        plan
    }
}

/// `analysis` of `plan`'s units, in the server's terms.
fn analysed(plan: &Plan, analysis: Analysis) -> Analysed {
    let Analysis {
        elaboration,
        files,
        origins,
        parsed,
        index,
        ..
    } = analysis;
    let units = (plan.units.iter().zip(files).zip(origins).zip(parsed))
        .map(|(((unit, file), origins), parsed)| Unit {
            path: unit.path.clone(),
            file,
            origins,
            parsed,
        })
        .collect();
    Analysed {
        elaboration,
        index: index.expect("a load summarizes every unit"),
        units,
    }
}

/// `problem` as plain text, without the marks a terminal styles.
fn plain(problem: &term::Problem) -> Problem {
    let plain = |said: &[String]| said.iter().map(|it| mark::plain(it)).collect();
    let at = problem.place().map(|(path, place, label)| At {
        path: path.to_path_buf(),
        place: match place {
            term::Place::Name(at, chars) => Place::Name(*at, *chars),
            term::Place::Line(line) => Place::Line(*line),
            term::Place::Bytes(bytes) => Place::Bytes(bytes.clone()),
        },
        label: label.map(mark::plain),
    });
    Problem {
        severity: problem.severity(),
        message: mark::plain(&problem.message),
        notes: plain(problem.notes()),
        help: plain(problem.helps()),
        at,
    }
}

/// `mutex`, held, whether or not a load panicked holding it: a plan is
/// inserted whole.
fn held<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|it| it.into_inner())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicBool, Ordering};

    use astli_lsp::{Host as _, Snapshot};
    use astli_testing::Fixture;
    use astli_text::{Disk, Reader};

    use super::Host;

    /// Buffers in front of the disk, as the server holds them.
    struct Buffers(HashMap<PathBuf, String>);

    impl Reader for Buffers {
        fn exists(&self, path: &Path) -> bool {
            self.0.contains_key(path) || Disk.exists(path)
        }

        fn read(&self, path: &Path) -> io::Result<String> {
            match self.0.get(path) {
                Some(text) => Ok(text.clone()),
                None => Disk.read(path),
            }
        }
    }

    #[test]
    fn a_package_is_loaded_through_the_buffers() {
        let fixture = Fixture::new("cli-host-load");
        fixture.file("astli.toml", "");
        let top = fixture.file("top.sv", "module top;\nendmodule\n");
        let text = "module top;\nendmodule\nmodule in_buffer;\nendmodule\n";
        let buffers = Buffers(HashMap::from([(top.clone(), text.to_string())]));
        let host = Host::default();
        let root = host.design(&top);
        assert_eq!(root, fixture.path());

        for discover in [true, false] {
            let snapshot = host.load(&root, &buffers, discover, &|| false).unwrap();
            assert_eq!(snapshot.root, root);
            assert!(snapshot.problems.is_empty(), "{:?}", snapshot.problems);
            assert_eq!((snapshot.designs.len(), snapshot.units()), (1, 1));
            let design = &snapshot.designs[0];
            assert_eq!(design.units[0].path, top);
            assert!(design.units[0].parsed.is_empty());
            let design = design.elaboration.design();
            assert!(design.definition("in_buffer").is_some());
        }

        // A load an edit overtakes makes nothing.
        assert!(host.load(&root, &buffers, false, &|| true).is_none());
    }

    #[test]
    fn a_manifest_that_does_not_parse_is_a_problem() {
        let fixture = Fixture::new("cli-host-unparsed");
        fixture.file("astli.toml", "[package\n");
        let top = fixture.file("top.sv", "module top;\nendmodule\n");
        let host = Host::default();
        let root = host.design(&top);
        assert_eq!(root, fixture.path());

        let snapshot = host.load(&root, &Disk, true, &|| false).unwrap();
        assert!(snapshot.designs.is_empty());
        let [problem] = &snapshot.problems[..] else {
            panic!("{:?}", snapshot.problems)
        };
        let at = problem.at.as_ref().expect("a place in the manifest");
        assert_eq!(at.path, fixture.path().join("astli.toml"));
    }

    /// The disk, holding the first read of a source until the test lets
    /// it go on.
    struct Holding {
        barrier: Barrier,
        first: AtomicBool,
    }

    impl Reader for Holding {
        fn exists(&self, path: &Path) -> bool {
            Disk.exists(path)
        }

        fn read(&self, path: &Path) -> io::Result<String> {
            // Only the first read waits, so that the others leave the pool
            // to what the test loads meanwhile.
            let source = path.extension().is_some_and(|it| it == "sv");
            if source && self.first.swap(false, Ordering::SeqCst) {
                self.barrier.wait();
                self.barrier.wait();
            }
            Disk.read(path)
        }
    }

    /// Loads the design at `root`, discovering it, on another thread, and
    /// holds the load at the first source it reads while `during` runs.
    fn held_while(host: &Host, root: &Path, during: impl FnOnce()) -> Snapshot {
        let holding = Holding {
            barrier: Barrier::new(2),
            first: AtomicBool::new(true),
        };
        std::thread::scope(|scope| {
            let held = scope.spawn(|| host.load(root, &holding, true, &|| false));
            holding.barrier.wait();
            during();
            holding.barrier.wait();
            held.join().expect("the load ends").expect("a snapshot")
        })
    }

    #[test]
    fn a_load_keeps_nothing_over_a_later_one_or_once_its_root_is_forgotten() {
        // Two threads at least, one for the load held and one for the rest.
        // The pool is built once a process, so under `cargo test` another
        // test may have built it already.
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build_global();
        let fixture = Fixture::new("cli-host-overlap");
        fixture.file("astli.toml", "");
        let module = |name: &str| format!("module {name};\nendmodule\n");
        let one = fixture.file("one.sv", &module("one"));
        fixture.file("two.sv", &module("two"));
        let host = Host::default();
        let root = host.design(&one);
        let load = |discover| host.load(&root, &Disk, discover, &|| false).unwrap();

        // What a later load discovered, the earlier one ending after it
        // does not replace.
        let earlier = held_while(&host, &root, || {
            fixture.file("three.sv", &module("three"));
            assert_eq!(load(true).units(), 3);
        });
        assert_eq!(earlier.units(), 2);
        assert_eq!(load(false).units(), 3);

        // A load ending after its root was forgotten keeps nothing.
        let earlier = held_while(&host, &root, || {
            host.forget(&root);
            fixture.file("four.sv", &module("four"));
        });
        assert_eq!(earlier.units(), 3);
        assert_eq!(load(false).units(), 4);
    }
}

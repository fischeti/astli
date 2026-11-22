//! What the language server asks of the driver: the layout a file is
//! formatted in, which design a file is in, and that design loaded as
//! `check` with no arguments analyses it, through the server's buffers.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use astli_fmt::Options;
use astli_lint::Config;
use astli_lsp::{Analysed, At, Place, Problem, Snapshot, Unit, Watched};
use astli_package::{BENDER_MANIFEST, EXTENSIONS, MANIFEST, mark};
use astli_resolve::LOCKFILE;
use astli_text::Reader;

use crate::analysis::{Analysis, Designs};
use crate::config::{FmtConfig, Levels, LintConfig};
use crate::discovery::{Plan, Whose};
use crate::error::{Error, Result};
use crate::listing;
use crate::run::Run;
use crate::target::Target;
use crate::term;
use crate::units::{Load, Units};

/// The levels `lint` reads each file at, by path.
type Leveled = HashMap<PathBuf, Arc<Config>>;

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
    /// The plan of the root's last discovery, with the problems it found
    /// and whether it read the package without its dependencies, which a
    /// load that does not discover again reports too; `None` if it failed,
    /// so that the next load discovers again and tells why.
    plan: (u64, Option<(Plan, Vec<term::Problem>, bool)>),
    /// The units the root's loads lowered, which the next load takes
    /// where what they read is unchanged.
    units: (u64, Arc<Units>),
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
        let before = (held(&self.roots).get(root))
            .map(|it| it.units.1.clone())
            .unwrap_or_default();
        let load = Load::new(before);
        let run = Run::loading(reader).abandoning(overtaken).keeping(&load);
        let (mut problems, mut levels) = (Vec::new(), HashMap::new());
        let analysed = self.analysed(root, &run, discover, ticket, &mut problems, &mut levels);
        let finished = analysed.is_ok();
        let designs = match analysed {
            Ok(designs) => designs,
            Err(Error::Overtaken) => {
                drop(run);
                self.keep(root, ticket, load, false);
                return None;
            }
            // Its problems are kept already.
            Err(Error::Silent) => Vec::new(),
            Err(err) => {
                err.problem().report(&run);
                Vec::new()
            }
        };
        problems.extend(run.problems());
        let lacks_dependencies = run.lacks_dependencies();
        drop(run);
        let lowered = load.count();
        self.keep(root, ticket, load, finished);
        Some(Snapshot {
            root: root.to_path_buf(),
            lowered,
            designs,
            problems: problems.iter().map(plain).collect(),
            lacks_dependencies,
            levels,
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
    fn kept<T>(&self, root: &Path, ticket: u64, keep: impl FnOnce(&mut Kept) -> T) -> Option<T> {
        let mut roots = held(&self.roots);
        let kept = roots.entry(root.to_path_buf()).or_default();
        (ticket >= kept.floor).then(|| keep(kept))
    }

    /// Keeps the units `load` used or lowered for the next load of `root`,
    /// unless a later load kept its own already: in place of those kept,
    /// if the load `finished`, so that a unit no load uses is not kept for
    /// ever; else over them, since a load that stopped short says nothing
    /// of the units it did not reach, and what it lowered is newer than
    /// what it took.
    fn keep(&self, root: &Path, ticket: u64, load: Load, finished: bool) {
        let used = load.into_used();
        let dropped = self.kept(root, ticket, |kept| match kept.units.0 > ticket {
            true => Some(Arc::new(used)),
            false => {
                kept.units.0 = ticket;
                match finished {
                    true => Some(std::mem::replace(&mut kept.units.1, Arc::new(used))),
                    false => {
                        Arc::make_mut(&mut kept.units.1).extend(used);
                        None
                    }
                }
            }
        });
        // Dropped here, on the pool, once the lock is let go.
        drop(dropped);
    }

    /// Each design `check` analyses of the design at `root`, adding what
    /// its discovery found to `problems`, and setting `levels` to those
    /// `lint` reads each file of the package at once the package is found,
    /// so that a design that fails still has them.
    fn analysed(
        &self,
        root: &Path,
        run: &Run,
        discover: bool,
        ticket: u64,
        problems: &mut Vec<term::Problem>,
        levels: &mut Leveled,
    ) -> Result<Vec<Analysed>> {
        let plan = self.plan(root, run, discover, ticket, problems)?;
        *levels = leveled(root, run, &plan);
        let levels = &*levels;
        let mut designs = Designs::of(&mut io::sink(), &plan, &[], false, &[], run)?;
        let analyse =
            |_: &mut _, plan, analysis, _: &mut _, _| analysed(run, &plan, analysis, levels);
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
            if let Some((plan, found, without)) = roots.get(root).and_then(|it| it.plan.1.as_ref())
            {
                problems.extend(found.iter().cloned());
                if *without {
                    run.read_without_dependencies();
                }
                return Ok(plan.clone());
            }
        }
        let plan = Plan::load(root, run);
        let found = plan.as_ref().ok().map(|plan| {
            let found = run.problems();
            problems.extend(found.iter().cloned());
            (plan.clone(), found, run.lacks_dependencies())
        });
        let dropped = self.kept(root, ticket, |kept| match kept.plan.0 > ticket {
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

/// The levels `lint` reads each of `plan`'s own files at, as `lint` in the
/// directory of `root` reads them, files at the same levels sharing them;
/// none if the configuration does not read, which is then the run's
/// problem.
fn leveled(root: &Path, run: &Run, plan: &Plan) -> Leveled {
    let target = Target::new(root.to_path_buf());
    let config = match LintConfig::load(run, &target, None, &Levels::default(), plan) {
        Ok(config) => config,
        Err(err) => {
            if !matches!(err, Error::Silent) {
                err.problem().report(run);
            }
            return HashMap::new();
        }
    };
    // A package's files mostly share their levels, and there are thousands
    // of them in a large one.
    let mut distinct: Vec<Arc<Config>> = Vec::new();
    (plan.own.iter())
        .map(|path| {
            let config = config.for_file(path);
            let shared = match distinct.iter().find(|it| ***it == config) {
                Some(shared) => shared.clone(),
                None => {
                    distinct.push(Arc::new(config));
                    distinct[distinct.len() - 1].clone()
                }
            };
            (path.clone(), shared)
        })
        .collect()
}

/// `analysis` of `plan`'s units, in the server's terms, with which of them
/// `check` and `lint`'s design rules report, as those commands choose
/// them, `levels` holding the files `lint` reads.
///
/// A name declared more than once is told as `check` tells it, unless the
/// package was read without its dependencies, whose files would declare
/// what it names; that it fails `check` is the command's to say.
fn analysed(run: &Run, plan: &Plan, analysis: Analysis, levels: &Leveled) -> Result<Analysed> {
    let names = analysis.top_names();
    let tops = analysis.bodies();
    let Analysis {
        elaboration,
        files,
        origins,
        parsed,
        recorded,
        index,
        ..
    } = analysis;
    let index = index.expect("a load summarizes every unit");
    let reached = match names.is_empty() {
        true => None,
        false => Some((index.reachable(&names)).map_err(|err| Error::failed(err.to_string()))?),
    };
    let checked = plan.shown(reached.as_deref(), Whose::OnDisk);
    if !run.lacks_dependencies() {
        let told = reached.clone().unwrap_or_else(|| checked.clone());
        let _ = listing::redeclared(run, &index, &told, &names);
    }
    let marked = |shown: Vec<usize>| {
        let mut marked = vec![false; plan.units.len()];
        for unit in shown {
            marked[unit] = true;
        }
        marked
    };
    let linted = marked(plan.shown(reached.as_deref(), Whose::Own));
    let checked = marked(checked);
    let units = (plan
        .units
        .iter()
        .zip(files)
        .zip(origins)
        .zip(parsed)
        .zip(recorded)
        .enumerate())
    .map(|(at, ((((unit, file), origins), parsed), recorded))| Unit {
        path: unit.path.clone(),
        file,
        origins,
        parsed,
        checked: checked[at],
        linted: linted[at] && levels.contains_key(&unit.path),
        calls: recorded.calls,
        skipped: recorded.skipped,
    })
    .collect();
    Ok(Analysed {
        elaboration,
        index,
        units,
        tops,
    })
}

/// `problem` as plain text, without the marks a terminal styles.
fn plain(problem: &term::Problem) -> Problem {
    let plain = |said: &[String]| said.iter().map(|it| mark::plain(it)).collect();
    let at = problem.place().map(|(code, path, place, label)| At {
        code: code.to_string(),
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
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};

    use astli_lint::Layer;
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
        let text = "`define W 1\nmodule top;\nendmodule\nmodule in_buffer;\n  \
                    localparam int P = `W;\n`ifdef NOPE\n  logic skipped;\n`endif\nendmodule\n";
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
            // What its expansion recorded is kept with the unit, the one
            // kept from the first load too.
            let unit = &design.units[0];
            let origins = &unit.origins;
            let calls: Vec<&str> = (unit.calls.iter())
                .map(|&it| origins.slice(origins.expansion(it).name))
                .collect();
            assert_eq!(calls, ["`W"]);
            let skipped: Vec<&str> = unit.skipped.iter().map(|&it| origins.slice(it)).collect();
            assert_eq!(skipped, ["\n  logic skipped;\n"]);
            let design = design.elaboration.design();
            assert!(design.definition("in_buffer").is_some());
        }

        // A load an edit overtakes makes nothing.
        assert!(host.load(&root, &buffers, false, &|| true).is_none());
    }

    #[test]
    fn a_load_lowers_again_only_the_units_that_read_a_change() {
        let fixture = Fixture::new("cli-host-units");
        let manifest = "[package]\nname = 'units'\ninclude-dirs = ['first', 'second']\n";
        fixture.file("astli.toml", manifest);
        fixture.dir("first");
        let header = fixture.file("second/a.svh", "`define WIDTH 8\n");
        let unit = |name: &str| format!("`include \"a.svh\"\nmodule {name};\nendmodule\n");
        let one = fixture.file("one.sv", &unit("one"));
        fixture.file("two.sv", &unit("two"));
        fixture.file("three.sv", "module three;\nendmodule\n");
        let host = Host::default();
        let root = host.design(&one);
        let mut buffers = Buffers(HashMap::new());
        let load = |buffers: &Buffers| {
            let snapshot = host.load(&root, buffers, false, &|| false).unwrap();
            assert!(snapshot.problems.is_empty(), "{:?}", snapshot.problems);
            assert_eq!(snapshot.units(), 3);
            snapshot
        };
        assert_eq!(load(&buffers).lowered, 3);

        let edited = "`include \"a.svh\"\nmodule edited;\nendmodule\n";
        buffers.0.insert(one.clone(), edited.to_string());
        let snapshot = load(&buffers);
        assert_eq!(snapshot.lowered, 1);
        let design = snapshot.designs[0].elaboration.design();
        assert!(design.definition("edited").is_some());
        assert!(design.definition("one").is_none());

        buffers.0.insert(header, "`define WIDTH 16\n".to_string());
        assert_eq!(load(&buffers).lowered, 2);
        assert_eq!(load(&buffers).lowered, 0);

        // Found earlier in the search, a header of the same name is what
        // the units that searched for it include now.
        fixture.file("first/a.svh", "`define WIDTH 32\n");
        assert_eq!(load(&buffers).lowered, 2);
        assert_eq!(load(&buffers).lowered, 0);
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

    #[test]
    fn a_load_says_what_check_and_lint_report_of_each_unit() {
        let fixture = Fixture::new("cli-host-reported");
        let manifest = "[package]\nname = 'reported'\n\n\
                        [dependencies]\nip = { path = 'ip' }\n\n[top.top]\n\n\
                        [lint.paths]\n'quiet.sv' = { allow = ['correctness'] }\n";
        fixture.file("astli.toml", manifest);
        fixture.file("ip/astli.toml", "[package]\nname = 'ip'\n");
        fixture.file("ip/ip.sv", "module ip;\nendmodule\n");
        let top = fixture.file(
            "top.sv",
            "module top;\n  ip u_ip ();\n  quiet u_quiet ();\nendmodule\n",
        );
        let quiet = fixture.file("quiet.sv", "module quiet;\nendmodule\n");
        let unused = fixture.file("unused.sv", "module unused;\nendmodule\n");
        let host = Host::default();
        let root = host.design(&top);

        let snapshot = host.load(&root, &Disk, true, &|| false).unwrap();
        assert!(snapshot.problems.is_empty(), "{:?}", snapshot.problems);
        assert!(!snapshot.lacks_dependencies);
        let [design] = &snapshot.designs[..] else {
            panic!("one design");
        };
        assert_eq!(design.tops.len(), 1);
        // Each unit's path, whether `check` reports it, and whether `lint`
        // does: the dependency's only `check`, since it is on disk, and the
        // file no top reaches neither.
        let mut reported: Vec<(String, bool, bool)> = (design.units.iter())
            .map(|unit| {
                let name = unit.path.strip_prefix(fixture.path()).unwrap();
                let name = name.to_string_lossy().replace('\\', "/");
                (name, unit.checked, unit.linted)
            })
            .collect();
        reported.sort();
        let reported: Vec<(&str, bool, bool)> = (reported.iter())
            .map(|(name, c, l)| (name.as_str(), *c, *l))
            .collect();
        assert_eq!(
            reported,
            [
                ("ip/ip.sv", true, false),
                ("quiet.sv", true, true),
                ("top.sv", true, true),
                ("unused.sv", false, false),
            ]
        );
        // The levels of each file of the package, those its pattern quiets
        // apart from the rest, which share theirs.
        let mut leveled: Vec<&PathBuf> = snapshot.levels.keys().collect();
        leveled.sort();
        assert_eq!(leveled, [&quiet, &top, &unused]);
        let levels = |path: &Path| &snapshot.levels[path];
        assert!(Arc::ptr_eq(levels(&top), levels(&unused)));
        assert!(levels(&top).needs(Layer::Design));
        assert_ne!(levels(&top), levels(&quiet));
    }

    #[test]
    fn a_load_without_its_dependencies_says_so_and_tells_no_name_declared_twice() {
        let fixture = Fixture::new("cli-host-without");
        let git = "[dependencies]\nip = { git = 'https://example.com/ip.git', version = '1' }\n";
        fixture.file("astli.toml", git);
        let one = fixture.file("one.sv", "module twice;\nendmodule\n");
        fixture.file("two.sv", "module twice;\nendmodule\n");
        let host = Host::default();
        let root = host.design(&one);
        let declared_twice = |snapshot: &Snapshot| {
            (snapshot.problems.iter())
                .any(|it| it.at.as_ref().is_some_and(|at| at.code == "declared-twice"))
        };

        // Kept with the plan, the dependencies are still missing from a load
        // that does not discover the package again.
        for discover in [true, false] {
            let snapshot = host.load(&root, &Disk, discover, &|| false).unwrap();
            assert!(snapshot.lacks_dependencies);
            assert!(!declared_twice(&snapshot), "{:?}", snapshot.problems);
            assert_eq!(snapshot.units(), 2);
        }

        fixture.file("astli.toml", "");
        let snapshot = host.load(&root, &Disk, true, &|| false).unwrap();
        assert!(!snapshot.lacks_dependencies);
        assert!(declared_twice(&snapshot), "{:?}", snapshot.problems);
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
        let later = load(false);
        assert_eq!((later.units(), later.lowered), (3, 0));

        // A load ending after its root was forgotten keeps nothing.
        let earlier = held_while(&host, &root, || {
            host.forget(&root);
            fixture.file("four.sv", &module("four"));
        });
        assert_eq!(earlier.units(), 3);
        let later = load(false);
        assert_eq!((later.units(), later.lowered), (4, 4));
    }
}

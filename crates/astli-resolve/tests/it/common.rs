//! Repositories the tests make with `git`, and a store to mirror them into.

use std::path::{Path, PathBuf};

use astli_package::Package;
use astli_resolve::{Located, Lock, Overrides, Store, resolve};

/// A temporary directory holding repositories and a store.
pub struct Fixture(astli_testing::Fixture);

impl Fixture {
    pub fn new(name: &str) -> Fixture {
        let dir = astli_testing::Fixture::new(name);
        // An empty file rather than `/dev/null`, which Windows lacks.
        dir.file("gitconfig", "");
        Fixture(dir)
    }

    pub fn path(&self) -> &Path {
        self.0.path()
    }

    pub fn store(&self) -> Store {
        Store::at(self.0.path().join("store"))
    }

    /// A new repository named `name`, its branch `main`.
    pub fn repo(&self, name: &str) -> Repo {
        let repo = Repo {
            dir: self.0.path().join(name),
            config: self.0.path().join("gitconfig"),
        };
        std::fs::create_dir_all(&repo.dir).expect("a directory for the repository");
        repo.git(&["init", "--quiet", "--initial-branch=main"]);
        repo
    }

    /// Writes `text` at `name` under the fixture, outside any repository.
    pub fn file(&self, name: &str, text: &str) -> PathBuf {
        self.0.file(name, text)
    }

    /// A root at `root/` under the fixture that requires `repo` at `commit`
    /// and nothing else.
    pub fn requiring(&self, root: &str, repo: &Repo, commit: &str) -> Package {
        let entry = format!("upstream = {{ git = '{}', rev = '{commit}' }}", repo.url());
        let manifest = self.file(
            &format!("{root}/astli.toml"),
            &format!("[dependencies]\n{entry}\n"),
        );
        Package::read(&manifest).expect("a valid manifest")
    }
}

/// What `root` requires, resolved and checked out into `store`, as a design
/// that reads it is.
pub fn checked_out(store: &Store, root: Package) -> Located {
    let resolution = resolve(
        store,
        &[root],
        &Overrides::default(),
        &Lock::default(),
        &Lock::default(),
    )
    .unwrap_or_else(|err| panic!("unresolved: {err}"));
    (resolution.packages[0].checkout(store)).expect("checked out")
}

/// A repository the test writes to; its path is its URL.
pub struct Repo {
    dir: PathBuf,
    config: PathBuf,
}

impl Repo {
    pub fn url(&self) -> String {
        self.dir.display().to_string()
    }

    /// What `git` prints for `args` in the repository, the user's and the
    /// system's configuration left out.
    pub fn git(&self, args: &[&str]) -> String {
        let output = (astli_testing::git(&self.dir, &self.config).args(args))
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("UTF-8")
            .trim()
            .to_string()
    }

    /// Writes `files` and commits them, returning the commit.
    pub fn commit(&self, files: &[(&str, &str)]) -> String {
        for (name, text) in files {
            let path = self.dir.join(name);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            std::fs::write(path, text).expect("a written file");
        }
        self.git(&["add", "--all"]);
        self.git(&["commit", "--quiet", "--allow-empty", "--message", "change"]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Commits `submodules`, each a path, the URL `.gitmodules` gives it
    /// and the commit its gitlink names, as `git submodule add` would
    /// without cloning one, returning the commit.
    pub fn submodules(&self, submodules: &[(&str, &str, &str)]) -> String {
        let modules = self.dir.join(".gitmodules");
        let mut text = std::fs::read_to_string(&modules).unwrap_or_default();
        for (path, url, commit) in submodules {
            // A `\` escapes what follows in git's configuration, and a
            // Windows path is full of them.
            let url = url.replace('\\', "\\\\");
            text += &format!("[submodule \"{path}\"]\n\tpath = {path}\n\turl = {url}\n");
            let gitlink = format!("160000,{commit},{path}");
            self.git(&["update-index", "--add", "--cacheinfo", &gitlink]);
        }
        std::fs::write(&modules, text).expect("a written file");
        self.git(&["add", ".gitmodules"]);
        self.git(&["commit", "--quiet", "--message", "submodules"]);
        self.git(&["rev-parse", "HEAD"])
    }

    /// Commits `files` and tags the commit `tag`, returning the commit.
    pub fn release(&self, tag: &str, files: &[(&str, &str)]) -> String {
        let commit = self.commit(files);
        self.git(&["tag", tag]);
        commit
    }
}

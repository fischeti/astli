//! Resolves a design that bender has resolved, and compares the two.
//!
//! ```text
//! cargo run --release -p astli-resolve --example bender-lock -- corpus/cheshire [--strict]
//! ```
//!
//! The root's dependencies come from its `Bender.yml`. bender lets the
//! root's requirement of a package decide, so each is pinned, unless
//! `--strict` asks what strict resolution makes of the design. Each package
//! is compared with what `Bender.lock` holds: the same commit, a newer
//! release than the lock's (it was written before that release), or else
//! both commits. Fetches into `ASTLI_CACHE_DIR`, or the user's cache.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use astli_package::Package;
use astli_resolve::{Origin, Store, canonical, resolve};
use serde::Deserialize;

#[derive(Deserialize)]
struct Lock {
    packages: BTreeMap<String, Locked>,
}

#[derive(Deserialize)]
struct Locked {
    revision: Option<String>,
    version: Option<String>,
    source: BTreeMap<String, String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let design = PathBuf::from(args.first().ok_or("which design?")?);
    let strict = args.iter().any(|it| it == "--strict");

    let root = Package::at(&design)?;
    let lock: Lock = serde_saphyr::from_str(&std::fs::read_to_string(design.join("Bender.lock"))?)?;
    let pinned: BTreeSet<String> = match strict {
        true => BTreeSet::new(),
        false => root.dependencies.keys().cloned().collect(),
    };

    let start = std::time::Instant::now();
    let none = astli_resolve::Lock::default();
    let resolution = resolve(&Store::from_env()?, &root, &pinned, &none)?;
    println!(
        "{}: {} packages in {:.1?}, {} warnings",
        design.display(),
        resolution.packages.len(),
        start.elapsed(),
        resolution.warnings.len()
    );
    for warning in &resolution.warnings {
        println!("  warning: {warning}");
    }

    let ours: BTreeMap<&str, _> = (resolution.packages.iter())
        .map(|it| (it.name.as_str(), it))
        .collect();
    let names: BTreeSet<&str> = (ours.keys().copied())
        .chain(lock.packages.keys().map(String::as_str))
        .collect();
    let (mut same, mut newer) = (0, 0);
    for name in names {
        let (Some(ours), Some(locked)) = (ours.get(name), lock.packages.get(name)) else {
            let only = if ours.contains_key(name) {
                "only astli's"
            } else {
                "only bender's"
            };
            println!("  {name:24} {only}");
            continue;
        };
        let locked_url = locked.source.get("Git").map(|url| canonical(url));
        if let Origin::Git { url } = &ours.origin
            && Some(canonical(url)) != locked_url
        {
            println!("  {name:24} from {url}, bender's from {:?}", locked.source);
            continue;
        }
        let commit = ours.commit.map(|it| it.to_string());
        if commit.is_some() && commit == locked.revision {
            same += 1;
            continue;
        }
        let version = ours.version.as_ref().map(ToString::to_string);
        let newer_release = match (&ours.version, &locked.version) {
            (Some(ours), Some(theirs)) => semver::Version::parse(theirs).is_ok_and(|it| *ours > it),
            _ => false,
        };
        if newer_release {
            newer += 1;
            println!(
                "  {name:24} {} newer than the lock's {}",
                version.unwrap_or_default(),
                locked.version.as_deref().unwrap_or_default()
            );
            continue;
        }
        let short = |it: Option<&str>| {
            it.map(|it| it[..10.min(it.len())].to_string())
                .unwrap_or_default()
        };
        println!(
            "  {name:24} {} {} against bender's {} {}",
            version.as_deref().unwrap_or("-"),
            short(commit.as_deref()),
            locked.version.as_deref().unwrap_or("-"),
            short(locked.revision.as_deref()),
        );
    }
    println!("  {same} the same commit, {newer} a newer release");
    Ok(())
}

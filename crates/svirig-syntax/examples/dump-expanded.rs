//! Prints a file with its macros expanded and its `` `include ``s followed,
//! and with `--origins` the provenance of every token that a macro placed.
//!
//! The third of the dumps. `dump-tokens` answers "how did this lex",
//! `dump-directives` answers "what did the preprocessor decide this is", and
//! this answers "what does it mean once the macros are gone" -- which is also
//! the output `slang -E` is held against.
//!
//!     cargo run --example dump-expanded -- file.sv -I include/ --origins

use std::path::PathBuf;
use std::process::ExitCode;

use svirig_syntax::preproc::{Includes, expand, render};
use svirig_text::{Disk, Origins};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_origins = args.iter().any(|arg| arg == "--origins");
    // `-Idir`, which is what every flow that has an include path writes it as.
    let quoted: Vec<PathBuf> = args
        .iter()
        .filter_map(|arg| arg.strip_prefix("-I"))
        .map(PathBuf::from)
        .collect();
    let Some(path) = args
        .iter()
        .find(|arg| !arg.starts_with("--") && !arg.starts_with("-I"))
    else {
        eprintln!("usage: dump-expanded <file.sv> [-I<dir>...] [--origins]");
        return ExitCode::FAILURE;
    };

    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!("{path}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let mut origins = Origins::new();
    let file = origins.add_file(path, contents);
    let includes = Includes {
        quoted,
        ..Includes::new()
    };
    let tokens = expand(&mut origins, file, &includes, &Disk);

    if !show_origins {
        print!("{}", render(&origins, &tokens));
        return ExitCode::SUCCESS;
    }

    // Only the tokens a macro placed. The rest are where the reader left them,
    // and printing those would bury the ones worth looking at.
    for token in tokens.iter().filter(|token| token.origin.from.is_some()) {
        let spelled = token.origin.spelled;
        let at = match origins.path(spelled.file) {
            Some(path) => format!(
                "{}:{}",
                path.display(),
                origins.line_col(spelled.file, spelled.start)
            ),
            // Pasted or stringified: the bytes are in no file.
            None => "<synthesised>".to_string(),
        };
        let through: Vec<_> = origins
            .trace(token.origin)
            .map(|expansion| origins.slice(expansion.name))
            .collect();

        println!(
            "{:<28} {:<16} {:<20} through {}",
            at,
            format!("{:?}", token.kind),
            format!("{:?}", origins.slice(spelled)),
            through.join(" < ")
        );
    }

    ExitCode::SUCCESS
}

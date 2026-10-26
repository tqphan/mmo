//! `contentgen write` regenerates the outputs from `content/`; `contentgen check` fails if they
//! are stale. Paths are relative to the repo root, which `cargo xtask` runs from.

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode != "write" && mode != "check" {
        eprintln!("usage: contentgen write|check");
        return ExitCode::from(2);
    }
    let content = match contentgen::Sources::read(Path::new("content")).and_then(|s| contentgen::parse(&s)) {
        Ok(c) => c,
        Err(errors) => {
            errors.iter().for_each(|e| eprintln!("content/{e}"));
            return ExitCode::FAILURE;
        }
    };
    let mut stale = false;
    for (path, text) in contentgen::render(&content) {
        let current = std::fs::read_to_string(path).map(|s| s.replace("\r\n", "\n")).ok();
        if current.as_deref() == Some(text.as_str()) {
            continue;
        }
        if mode == "check" {
            eprintln!("{path} is stale; run `cargo xtask content`");
            stale = true;
        } else if let Err(e) = std::fs::write(path, text) {
            eprintln!("{path}: {e}");
            return ExitCode::FAILURE;
        } else {
            eprintln!("wrote {path}");
        }
    }
    if stale { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

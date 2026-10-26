//! Dev tasks, cross-platform (Linux and Windows). Run from the repo root: `cargo xtask <task>`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const DB: &str = "mmo-world";
const WASM: &str = "target/wasm32-unknown-unknown/release/world.wasm";

const HELP: &str = "\
cargo xtask <task> [options]

  test                 unit and module tests, then clippy with warnings as errors
  build [--prod]       build the world module to wasm (dev build has the test clock)
  server               run a local SpacetimeDB server in the foreground (data in target/stdb-data)
  publish [--prod] [--clear]
                       build and publish to the local server; --clear wipes the database
  generate             regenerate client bindings (Rust for loadbot, C# for the Godot client)
  bot <scenario>       run a loadbot scenario against the local server
  check                test, then publish --clear and run every bot scenario (needs `server`)
";

const SCENARIOS: &[&str] = &["see-each-other", "tick"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    std::env::set_current_dir(&root).unwrap();

    let result = match args.first().map(String::as_str) {
        Some("test") => test(),
        Some("build") => build(flag("--prod")),
        Some("server") => server(&root),
        Some("publish") => publish(flag("--prod"), flag("--clear")),
        Some("generate") => generate(),
        Some("bot") => match args.get(1) {
            Some(s) => bot(s),
            None => Err(format!("scenarios: {}", SCENARIOS.join(", "))),
        },
        Some("check") => check(),
        _ => {
            print!("{HELP}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

type Task = Result<(), String>;

fn run(cmd: &mut Command) -> Task {
    eprintln!("$ {cmd:?}");
    let status = cmd.status().map_err(|e| format!("could not start {:?}: {e}", cmd.get_program()))?;
    if status.success() { Ok(()) } else { Err(format!("{:?} failed: {status}", cmd.get_program())) }
}

fn cargo() -> Command {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

/// The SpacetimeDB CLI: `$SPACETIME`, then PATH, then the installer's default location.
fn spacetime() -> Command {
    if let Ok(p) = std::env::var("SPACETIME") {
        return Command::new(p);
    }
    let exe = if cfg!(windows) { "spacetime.exe" } else { "spacetime" };
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
        .unwrap_or(false);
    if on_path {
        return Command::new(exe);
    }
    let default: Option<PathBuf> = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("SpacetimeDB").join(exe))
    } else {
        std::env::var_os("HOME").map(|d| PathBuf::from(d).join(".local/bin").join(exe))
    };
    Command::new(default.filter(|p| p.is_file()).unwrap_or_else(|| exe.into()))
}

/// Put a per-user .NET install (`$DOTNET_ROOT`, else `~/.dotnet`) on `cmd`'s PATH, so tools that
/// run `dotnet` themselves find it. A system-wide install is already on PATH.
fn with_dotnet(cmd: &mut Command) -> &mut Command {
    let exe = if cfg!(windows) { "dotnet.exe" } else { "dotnet" };
    let root = std::env::var_os("DOTNET_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".dotnet")))
        .filter(|d| d.join(exe).is_file());
    if let Some(root) = root {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let paths = std::iter::once(root.clone()).chain(std::env::split_paths(&path));
        cmd.env("DOTNET_ROOT", &root).env("PATH", std::env::join_paths(paths).unwrap());
    }
    cmd.env("DOTNET_CLI_TELEMETRY_OPTOUT", "1").env("DOTNET_NOLOGO", "1")
}

fn test() -> Task {
    run(cargo().args(["test", "--workspace", "--features", "world/dev"]))?;
    for features in [&["--features", "world/dev"][..], &[]] {
        run(cargo().args(["clippy", "--workspace", "--all-targets"]).args(features).args(["--", "-D", "warnings"]))?;
    }
    Ok(())
}

fn build(prod: bool) -> Task {
    let mut cmd = cargo();
    cmd.args(["build", "-p", "world", "--target", "wasm32-unknown-unknown", "--release"]);
    if !prod {
        cmd.args(["--features", "dev"]);
    }
    run(&mut cmd)
}

fn server(root: &Path) -> Task {
    let data = root.join("target").join("stdb-data");
    run(spacetime().args(["start", "--listen-addr", "127.0.0.1:3000", "--non-interactive", "--data-dir"]).arg(data))
}

fn publish(prod: bool, clear: bool) -> Task {
    build(prod)?;
    let mut cmd = spacetime();
    cmd.args(["publish", DB, "--server", "local", "--bin-path", WASM, "--yes"]);
    if clear {
        cmd.arg("--delete-data");
    }
    run(&mut cmd)
}

fn generate() -> Task {
    build(false)?;
    run(spacetime().args(["generate", "--lang", "rust", "--out-dir", "sidecars/loadbot/src/module_bindings"])
        .args(["--bin-path", WASM, "--yes"]))?;
    run(with_dotnet(&mut spacetime())
        .args(["generate", "--lang", "csharp", "--out-dir", "client/net"])
        .args(["--bin-path", WASM, "--yes"]))
}

fn bot(scenario: &str) -> Task {
    run(cargo().args(["run", "--quiet", "-p", "loadbot", "--", scenario]))
}

fn check() -> Task {
    test()?;
    publish(false, true)?;
    SCENARIOS.iter().try_for_each(|s| bot(s))
}

//! Dev tasks, cross-platform (Linux and Windows). Run from the repo root: `cargo xtask <task>`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const DB: &str = "mmo-world";
const WASM: &str = "target/wasm32-unknown-unknown/release/world.wasm";

const HELP: &str = "\
cargo xtask <task> [options]

  content              regenerate the content outputs (Rust statics, C# data, hash) from content/,
                       and the client's test vectors from sim
  test                 content check, unit and module tests, the Godot client's C# build and
                       xUnit tests, then clippy with warnings as errors
  build [--prod]       build the world module to wasm (dev build has the test clock)
  server               run a local SpacetimeDB server in the foreground (data in target/stdb-data)
  publish [--prod] [--clear]
                       build and publish to the local server; --clear wipes the database
  generate             regenerate client bindings (Rust for loadbot, C# for the Godot client)
  world [--gen]        upload the world's files (world/) to the local server as the publisher;
                       --gen first rewrites world/ with the placeholder generator (commit it)
  bot <scenario> [args]
                       run a loadbot scenario against the local server; `bot help` lists them
                       (`bot swarm --bots N --combat F --area M --secs S` is the throughput spike)
  check                test, then publish --clear and run every bot scenario (needs `server`)
  client [args]        build and run the Godot client; args go to it (`--profile NAME`,
                       `--autopilot circle:X,Z,R`, …; docs/design/client.md)
  capture <name> <frames> [args]
                       run the client in Movie Maker mode at 30 fps for <frames> frames, writing
                       PNGs to target/captures/<name>/; args go to the client
";

const SCENARIOS: &[&str] = &["see-each-other", "tick", "cross-cells", "terrain", "ship", "labor", "items", "gather", "craft"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    std::env::set_current_dir(&root).unwrap();

    let result = match args.first().map(String::as_str) {
        Some("content") => contentgen("write").and_then(|()| vectors()),
        Some("test") => test(),
        Some("build") => build(flag("--prod")),
        Some("server") => server(&root),
        Some("publish") => publish(flag("--prod"), flag("--clear")),
        Some("generate") => generate(),
        Some("world") => world(flag("--gen")),
        Some("bot") => match args.get(1) {
            Some(s) => bot(s, &args[2..]),
            None => Err(format!("scenarios: {}", SCENARIOS.join(", "))),
        },
        Some("check") => check(),
        Some("client") => client(&args[1..]),
        Some("capture") => match (args.get(1), args.get(2).and_then(|f| f.parse::<u32>().ok())) {
            (Some(name), Some(frames)) => capture(name, frames, &args[3..]),
            _ => Err("usage: capture <name> <frames> [client args]".into()),
        },
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

fn contentgen(mode: &str) -> Task {
    run(cargo().args(["run", "--quiet", "-p", "contentgen", "--", mode]))
}

/// Rewrite the client's golden test vectors from `sim` (`client/tests/vectors`).
fn vectors() -> Task {
    run(cargo().args(["test", "--quiet", "-p", "sim", "--lib", "vectors"]).env("MMO_WRITE_VECTORS", "1"))
}

/// Godot 4.7 .NET: `$GODOT`, then `godot` on PATH, then `~/.local/bin/godot` (Linux) or
/// `%LOCALAPPDATA%\Godot\godot.exe` (Windows).
fn godot() -> Command {
    if let Ok(p) = std::env::var("GODOT") {
        return Command::new(p);
    }
    let exe = if cfg!(windows) { "godot.exe" } else { "godot" };
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
        .unwrap_or(false);
    if on_path {
        return Command::new(exe);
    }
    let default: Option<PathBuf> = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Godot").join(exe))
    } else {
        std::env::var_os("HOME").map(|d| PathBuf::from(d).join(".local/bin").join(exe))
    };
    Command::new(default.filter(|p| p.is_file()).unwrap_or_else(|| exe.into()))
}

/// Build the Godot project's C# (bindings, content and client), warnings as errors.
fn client_build() -> Task {
    run(with_dotnet(&mut Command::new("dotnet")).args(["build", "--nologo", "-v", "quiet", "client/Mmo.csproj"]))
}

/// The client's xUnit tests: the Godot-free code, and the golden vectors from `sim`.
fn client_test() -> Task {
    run(with_dotnet(&mut Command::new("dotnet")).args(["test", "--nologo", "-v", "quiet", "client/tests/Mmo.Tests.csproj"]))
}

/// Import new and changed assets headless before a run; a run doesn't import them itself. About 2 s
/// when nothing changed.
fn client_import() -> Task {
    run(godot().args(["--headless", "--path", "client", "--import"]))
}

/// The SpacetimeDB CLI's token: the identity that publishes, which the module makes an admin.
fn publisher_token() -> Result<String, String> {
    let out = spacetime().args(["login", "show", "--token"]).output().map_err(|e| format!("spacetime login show: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    // "Your auth token (don't share this!) is <token>"
    text.lines()
        .find(|l| l.contains("auth token"))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_owned)
        .ok_or_else(|| "no token from `spacetime login show --token`; log in with the CLI first".into())
}

/// Upload `world/` as the publisher (docs/design/world.md), after rewriting it with the placeholder
/// generator if asked (a release build: seconds rather than minutes).
fn world(regenerate: bool) -> Task {
    if regenerate {
        run(cargo().args(["run", "--quiet", "--release", "-p", "worldtool", "--", "gen"]))?;
    }
    // The token goes in the environment, not on the command line, so it doesn't show in a process list.
    let token = publisher_token()?;
    eprintln!("$ loadbot world-upload (as the publisher)");
    let status = cargo()
        .args(["run", "--quiet", "-p", "loadbot", "--", "world-upload"])
        .env("MMO_ADMIN_TOKEN", token)
        .status()
        .map_err(|e| format!("could not start loadbot: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("world-upload failed: {status}")) }
}

fn client(extra: &[String]) -> Task {
    client_build()?;
    client_import()?;
    run(with_dotnet(&mut godot()).args(["--path", "client", "--"]).args(extra))
}

fn capture(name: &str, frames: u32, extra: &[String]) -> Task {
    client_build()?;
    client_import()?;
    let dir = std::env::current_dir().unwrap().join("target").join("captures").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    run(with_dotnet(&mut godot())
        .args(["--path", "client", "--fixed-fps", "30", "--quit-after", &frames.to_string(), "--write-movie"])
        .arg(dir.join("frame.png"))
        .arg("--")
        .args(extra))?;
    eprintln!("frames in {}", dir.display());
    Ok(())
}

fn test() -> Task {
    contentgen("check")?;
    run(cargo().args(["test", "--workspace", "--features", "world/dev"]))?;
    client_build()?;
    client_test()?;
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

/// Extra arguments go to loadbot. The swarm runs a release build, so the bots' own overhead
/// doesn't skew what it measures.
fn bot(scenario: &str, extra: &[String]) -> Task {
    let mut cmd = cargo();
    cmd.args(["run", "--quiet", "-p", "loadbot"]);
    if scenario == "swarm" {
        cmd.arg("--release");
    }
    run(cmd.args(["--", scenario]).args(extra))
}

fn check() -> Task {
    test()?;
    publish(false, true)?;
    SCENARIOS.iter().try_for_each(|s| bot(s, &[]))
}

# 0008: Two dev machines; dev tasks via `cargo xtask`; Cargo workspace at the repo root

- **Status:** Accepted, 2026-10-25 (user chose "both")
- **Context:** The plan assumed one Windows 11 PC. Work also happens on an Arch Linux laptop. Shell scripts would have to be written twice, as Bash and PowerShell.
- **Decision:**
  - Development happens on both machines, and every tool and script must work on both.
  - Dev tasks are a Rust program, `xtask/`, run as `cargo xtask <task>` from the repo root. Rust is the one toolchain both machines are guaranteed to have.
  - The Cargo workspace lives at the **repo root**, not under `server/` as PLAN §4.8 sketched. It covers `server/world`, `server/sim`, `sidecars/*` and `xtask`, so there's one `Cargo.lock` (which matters for ADR 0009) and one `target/`.
  - The xtask finds the `spacetime` CLI from `$SPACETIME`, then `PATH`, then the installer's default location.
- **Consequences:** New tools go into the xtask, not into shell scripts.

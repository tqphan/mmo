# 0005: Claude builds, the user directs; the repository is the memory

- **Status:** Accepted, 2026-10-25 (user)
- **Decision:**
  - Claude writes specs, code, tests, tools and procedural assets across many sessions. The user sets direction, approves installs, money and big decisions, and playtests.
  - Continuity lives in the repo: `CLAUDE.md`, `docs/STATUS.md`, `docs/BACKLOG.md`, ADRs, specs.
  - **Standing approval (2026-10-25):** Claude may commit and push to `main` on https://github.com/tqphan/mmo after each finished task without asking.
- **Consequences:** Every task ends with docs updated and a commit. Commits are small and self-describing.

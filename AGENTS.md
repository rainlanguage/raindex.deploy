# Repository Guidelines

- Prefer syntax-aware search with ast-grep: `sg --lang rust -p '<pattern>'` (TS:
  `--lang ts`).
- Read the nearest `ARCHITECTURE.md` before changing a directory.
- Path-scoped rules live in `.claude/rules/` (workflows, crates, build scripts)
  — check them before touching those areas.

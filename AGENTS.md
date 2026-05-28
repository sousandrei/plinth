# Julius Desktop — Agent Development Guide

Standards and workflows for developing the `desktop/` Tauri app.
The app has two distinct layers — a **Rust backend** (`src-tauri/`) and a
**React frontend** (`src/`) — each with its own toolchain.

The `client/`, `server/`, and `models/` directories are **reference only**.
Never modify them.

---

## Working Rules

These rules are non-negotiable and apply to every task:

1. **One step at a time.** Never start a task without being explicitly told to.
   Complete exactly one step, then stop and wait for confirmation before
   proceeding to the next. Do not speculatively implement the next step.

2. **One file at a time.** Do not write or edit multiple files in a single
   response unless the step explicitly requires it (e.g. a module file and its
   `mod.rs` registration). When in doubt, do less.

3. **Build and check after every step.** After completing any code change,
   always run the relevant build/lint commands and fix all errors before
   declaring the step done. A step is not complete until the build is green.

4. **Never push.** Do not run `git push` under any circumstance — not even
   with `--dry-run`. Pushing is always a manual human action.

5. **Never commit.** Do not run `git add`, `git commit`, or any wrapper around
   them. Staging and committing is always a manual human action. After a step
   is complete and the build is green, stop and wait for the human to review
   and commit before proceeding.

6. **Stop after every completed step.** One logical unit of work per step.
   Stop and wait for explicit instruction before starting the next step.

---

## Build & Check Commands

We have a central `Taskfile.yml` in the root of the project to coordinate development workflows. It is highly recommended to use the `task` runner.

```bash
# --- Standard Tasks (run from root directory) ---
task lint                              # Runs both frontend and backend linter/typecheck suites
task format                            # Runs auto-formatters globally for both layers
task sqlx:prepare                      # Regenerates SQLx offline metadata JSON cache files
task package                           # Builds and packages the production desktop app
```

If you need to run individual layer commands directly:

```bash
# --- Rust (run from tauri/) ---
cargo fmt --all                        # format
cargo clippy -- -D warnings            # lint — warnings are errors
cargo check                            # type-check without full build
cargo tauri dev                        # dev mode
cargo tauri build                      # release bundle

# --- Frontend (run from ui/) ---
bun run dev                            # dev server
bun run build                          # production build
bun run typecheck                      # tsc --noEmit
bun run check                          # Biome lint check
bun run format                         # Biome auto-fix
```

**Minimum required before any commit:**
```bash
task lint
```

---

## Git Rules

- **Conventional commits** — follow the style of existing repo history exactly:
  - Format: `type: short description` (no scope brackets unless genuinely needed)
  - All lowercase, no trailing period, imperative mood
  - Types used in this repo: `feat`, `fix`, `chore`, `wip`
  - Examples from history: `feat: better transactions page`,
    `fix: better seb month aggregation`, `chore: cleanup dependencies`
  - Use `wip:` only for partial commits that will be followed up immediately
- **Never commit:**
  - Generated files: `routeTree.gen.ts`, `target/`, `node_modules/`
  - Database files: `*.db`, `*.db-wal`, `*.db-shm`
  - Secrets or credentials of any kind
  - Files from multiple unrelated steps in one commit

---

## Rust (`src-tauri/`)

### Code style

- **No AI-oriented comments.** Comments explain *why* code does something, not
  *what* it does for a reader unfamiliar with the codebase. Never write comments
  like "// Called once during Tauri setup before any command is registered" or
  "// Learn more about Tauri commands at ...". Delete boilerplate comments left
  by scaffolding tools.
- **No `unwrap()` or `expect()` in command handlers.** Use `?` with `AppError`.
  Reserve `expect()` only for truly infallible invariants in `main.rs` startup.
- **Return `Result<T, AppError>` from every `#[tauri::command]`.**
  `AppError` implements `serde::Serialize` so Tauri serialises it cleanly.
- **Derive `Debug, Serialize, Deserialize, Clone`** on all structs in `models.rs`
  that cross the IPC boundary.
- **One command group per file** under `commands/`. Register all commands in
  `lib.rs`'s `generate_handler!` macro — never in `main.rs`.
- **State:** wrap shared state in `Arc<Mutex<T>>` and register via `.manage()`.
  Commands receive it via `tauri::State<'_, T>`.
- **No `std::thread::sleep` in async commands.** Use `tokio::time::sleep`.
- **Spawn blocking DB work** with `tokio::task::spawn_blocking` —
  `rusqlite` is synchronous; never block the async runtime directly.
- **SQL:** write raw SQL with positional parameters (`?1`, `?2` …).
  Keep queries in the command file they belong to.
- **Error context:** wrap foreign errors with a short message:
  ```rust
  db.query(...).map_err(|e| AppError::Db(format!("list_transactions: {e}")))?;
  ```

### AppError pattern

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]    Db(String),
    #[error("not found: {0}")]         NotFound(String),
    #[error("invalid input: {0}")]     InvalidInput(String),
    #[error("io error: {0}")]          Io(String),
    #[error("internal error: {0}")]    Internal(String),
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
```

### SQLite / migrations

- Migration files: `src-tauri/migrations/V{n}__{snake_case}.sql` (two underscores).
- **Never edit an existing migration.** Add a new versioned file instead.
- Enable foreign keys and WAL on every connection:
  ```rust
  conn.execute_batch(
      "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;"
  )?;
  ```
- `INTEGER` for booleans (`0`/`1`), `TEXT` for dates (ISO 8601), `INTEGER` for
  monetary amounts (minor units — öre/cents ×100).
- **FTS5** for full-text search (`transactions_fts`). Never use `LIKE` for search.
- **Upserts** via `INSERT ... ON CONFLICT DO UPDATE` for idempotent re-imports.
- **Never write inline SQL strings in Rust code.** All queries must live in `.sql`
  files under `tauri/queries/` and be referenced via `sqlx::query_file!` or
  `sqlx::query_file_as!`. After adding or changing any query file, run
  `task sqlx:prepare` to regenerate the offline metadata before building.

### Tauri command checklist

1. Add function in `commands/*.rs` with `#[tauri::command]`.
2. Register in `lib.rs` `generate_handler![...]`.
3. Add Tauri capability in `capabilities/default.json` if a plugin is used.
4. Add `invoke()` wrapper in `src/api/`.
5. Run `cargo fmt --all && cargo clippy -- -D warnings`.

---

## React / TypeScript (`src/`)

### Code style

- **Functional components only:** `const Component = (): JSX.Element => { }`.
- **Explicit prop interfaces** defined above the component.
- **No `any`.** Use `unknown` and narrow, or define proper types.
- **Path alias `@/`** for `src/` — never use relative `../../` imports.
- **Single quotes**, trailing commas, semicolons — enforced by Biome.
- **Organise imports** automatically via Biome's `organizeImports: on`.

### Tauri IPC wrappers

All `invoke()` calls live in `src/api/` — never directly in components.

```typescript
// src/api/users.ts
import { invoke } from '@tauri-apps/api/core';
import type { User } from '@/types';

export const listUsers = (): Promise<User[]> => invoke<User[]>('list_users');
export const createUser = (name: string): Promise<User> =>
  invoke<User>('create_user', { name });
```

### Types

- TypeScript types for every Rust struct that crosses IPC live in `src/types/`.
- Keep field names `snake_case` to match Tauri's JSON serialisation exactly.

### State and data fetching

- **TanStack Query** (`useQuery` / `useMutation`) for all Tauri command calls.
- Invalidate queries in `onSuccess` — never manually refresh.
- **TanStack Router** for navigation. Never edit `routeTree.gen.ts` manually.

### Styling

- **Tailwind CSS 4** exclusively. No inline `style={{}}` except for dynamic
  values that can't be Tailwind classes (e.g. OKLCH colors).
- `cn()` helper from `@/lib/util` for conditional class merging.
- Dark mode via `dark:` modifier on every component.
- **shadcn/ui** — add with `bunx --bun shadcn@latest add [component]`.
- **Magic UI** — add with `bunx --bun shadcn@latest add @magicui/[component]`.

### Component organisation

| Directory | Contents |
|---|---|
| `src/components/ui/` | shadcn/ui primitives |
| `src/components/magic/` | Magic UI animated components |
| `src/components/shared/` | App-wide shared components |
| `src/components/[feature]/` | Feature-scoped components |
| `src/hooks/` | Custom hooks (extract when logic exceeds ~15 lines) |
| `src/routes/` | TanStack Router file-based routes |
| `src/api/` | Tauri `invoke()` wrappers |
| `src/types/` | TypeScript types matching Rust models |
| `src/lib/` | Pure utilities (`util.ts`, etc.) |

---

## SQLite Database Rules

- Schema changes = new migration file. Never ALTER existing migrations.
- Foreign keys enabled at connection time (see `db.rs`).
- Monetary values as `INTEGER` minor units (×100); convert at UI layer only.
- Dates as `TEXT` ISO 8601 (`YYYY-MM-DD`); sorts lexicographically.
- Booleans as `INTEGER` (`0` = false, `1` = true).
- Full-text search via `transactions_fts` FTS5 — never `LIKE` for search.
- WAL mode enabled at startup.
- Upserts use `INSERT ... ON CONFLICT DO UPDATE`.

---

## Parser Scripts (`models/parsers/`)

- Each parser reads a file path from `argv[1]`, prints one JSON object to stdout.
- Nothing else to stdout — debug output goes to stderr.
- Exit `0` on success, non-zero on failure.
- Output conforms to the protocol in `PLAN.md §2.6`.
- New parsers: add entry to `registry.json` — no Rust changes required.

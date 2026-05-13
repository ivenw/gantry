# Skills Support — Implementation Plan

## Context

Gantry currently supports context files (`AGENTS.md`, `CLAUDE.md`) that are injected wholesale into
the system prompt. Skills are a richer, lazily-loaded variant: each skill lives in its own directory
with a `SKILL.md` file, carries structured metadata (name + description), and is only fully loaded
into context when the model decides it is relevant. This implements the
[Agent Skills spec](https://agentskills.io/client-implementation/adding-skills-support).

The initial implementation covers:
- **Tier 1**: catalog disclosure at session start (name + description + path, ~50-100 tokens/skill)
- **Tier 2**: file-read activation — the model reads `SKILL.md` directly using the existing `ReadTool`
- Deduplication and user-explicit activation (`/skill-name`) are out of scope for this iteration.

---

## Scan locations

Mirror the same global-first, project-walk ordering used by context files:

| Priority | Path | Rationale |
|----------|------|-----------|
| 1 | `~/.gantry/skills/` | Gantry-native user scope |
| 2 | `~/.agents/skills/` | Cross-client interoperability |
| 3 | `<project>/.gantry/skills/` | Gantry-native project scope |
| 4 | `<project>/.agents/skills/` | Cross-client project scope |

Within each `skills/` directory, a skill is any **subdirectory** containing a file named exactly
`SKILL.md`. `.git/` and `node_modules/` are skipped.

**Collision rule**: project-level skills shadow user-level skills with the same `name`. Within the
same scope, first-found wins. Log a warning when a collision occurs.

---

## New types — `gantry-core/src/resource_loader.rs`

```rust
/// Parsed metadata from a SKILL.md file's YAML frontmatter.
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

/// A discovered skill with its parsed metadata and the absolute path to its SKILL.md.
pub struct Skill {
    pub metadata: SkillMetadata,
    /// Absolute path to the SKILL.md file.
    pub skill_file: PathBuf,
}
```

---

## New functions — `gantry-core/src/resource_loader.rs`

### `load_skills(project_root) -> Result<Vec<Skill>>`

Top-level entry point, called at session start alongside `load_context_files`. Returns skills
deduplicated by name (project overrides user), ordered user-global first then project-level. Steps:

1. Collect candidate skill dirs in priority order (see table above).
2. Call `scan_skills_dir` on each, accumulate results into a `HashMap<String, Skill>` keyed by name,
   with later (higher-priority) entries overwriting earlier ones — but log a warning on collision.
3. Return `HashMap::into_values().collect()`.

### `scan_skills_dir(dir: &Path) -> Vec<Skill>`

Reads one `skills/` directory. For each immediate subdirectory, checks for `SKILL.md` and calls
`parse_skill_file`. Skips `.git` and `node_modules`. Returns successfully parsed skills; logs and
skips failures.

### `parse_skill_file(path: &Path) -> Result<Skill>`

Reads a `SKILL.md` file, splits on the `---` frontmatter delimiters, parses the YAML block with
`serde_yaml` to extract `name` and `description`. Returns an error (causing the skill to be skipped)
if:
- YAML is completely unparseable
- `description` is missing or empty

Warns but still loads if `name` doesn't match the parent directory name.

**Lenient YAML fallback**: if initial parse fails, retry after wrapping bare `description:` values
that contain colons in double quotes (regex replace `^(description:\s*)(.+:.+)$` →
`$1"$2"`). This improves cross-client compatibility.

---

## System prompt changes — `gantry-core/src/system_prompt.rs`

### `build_system_prompt(agent_files, skills) -> String`

Extend the existing signature to also accept `&[Skill]`. After the existing `# Context` section,
append a `# Skills` section containing:

1. A brief instruction block (file-read activation variant from the spec):
   ```
   The following skills provide specialized instructions for specific tasks.
   When a task matches a skill's description, use your file-read tool to load
   the SKILL.md at the listed location before proceeding.
   When a skill references relative paths, resolve them against the skill's
   directory (the parent of SKILL.md) and use absolute paths in tool calls.
   ```
2. An `<available_skills>` XML catalog block, one `<skill>` entry per skill with `name`,
   `description`, and `location`.

If `skills` is empty, omit the `# Skills` section entirely.

---

## App integration — `gantry-core/src/app.rs`

### `App` struct

Add two new fields alongside `agent_file_char_counts`:
```rust
skills: Vec<Skill>,
skill_char_count: usize,   // total chars of the catalog, for CharCounts
```

### `build_system_prompt_with_counts`

1. Call `load_skills(&project_root)` (`.unwrap_or_default()` like context files).
2. Pass skills to the updated `build_system_prompt`.
3. Capture `skill_char_count` as the sum of all name+description lengths in the catalog.

### `CharCounts` — `gantry-core/src/metrics.rs`

Add a `skills_catalog: usize` field to `CharCounts`, populated in `prepare_request`.

### `refresh_system_prompt`

Call `load_skills` again to pick up changes on disk (same pattern as context files).

---

## `dirs.rs` — project-level skills dir

`ProjectConfigDir` currently has no `skills_dir()` method. Add:

```rust
pub fn skills_dir(&self) -> PathBuf {
    self.0.join(SKILL_DIR)  // reuse existing SKILL_DIR constant
}
```

Also add a corresponding `skills_dir()` to `ProjectRootDir` (pointing to `.agents/skills/` inside
the project root) to cover the cross-client convention path.

---

## Dependencies

Add `serde_yaml` to `gantry-core/Cargo.toml`:

```toml
serde_yaml = "0.9"
```

---

## File-read permission allowlisting

Skill directories should be automatically trusted so the model can read bundled resources without
triggering permission prompts. This is handled by ensuring the existing `ReadTool` covers any path —
which it already does — so no changes needed here.

---

## Deferred / future work

### Deduplication across activations (requires dedicated skill tool)
Track which `SKILL.md` paths have already been injected into the conversation. On re-activation,
skip re-injection. This requires a dedicated `activate_skill` tool (not raw `ReadTool`) so the
harness can intercept the call and check an activation registry. Relevant spec section: Step 4 —
"Dedicated tool activation" and "Deduplicate activations".

### User-explicit activation (`/skill-name` syntax)
Intercept slash commands in the TUI input layer before they reach the model. Look up the skill by
name, read its `SKILL.md`, and inject the content as a system or user message. Relevant spec
section: Step 4 — "User-explicit activation".

### Context compaction protection
Once gantry gains context-window truncation/summarisation, skill tool outputs should be flagged as
protected so they survive pruning. Relevant spec section: Step 5 — "Protect skill content from
context compaction".

---

## Verification

1. Create `~/.gantry/skills/test-skill/SKILL.md` with valid frontmatter (`name`, `description`) and
   a body.
2. `cargo build -p gantry-core` — must compile clean.
3. Launch gantry on any project; inspect the system prompt (via a debug print or the context view) —
   the `# Skills` section and `<available_skills>` block must appear.
4. In a chat session, ask the model to perform a task matching the skill's description; verify it
   calls `ReadTool` with the correct `SKILL.md` path.
5. Add a project-level skill with the same name; verify the project one wins (check logged warning).
6. Create a malformed `SKILL.md` (empty description); verify gantry still starts and the skill is
   skipped with a logged warning.
7. `cargo test -p gantry-core` — all existing tests must pass; add unit tests for
   `parse_skill_file` covering: valid frontmatter, missing description, unparseable YAML, lenient
   colon-in-description fallback.

# Reasons: Rust Implementation Plan

A Rust port of [ftl-reasons](https://github.com/benthomasson/ftl-reasons) — a Truth Maintenance System (TMS) for managing justified beliefs with dependency tracking, contradiction detection, and truth-value propagation.

**Goal:** Single static binary, zero runtime dependencies, installable via `cargo install`, Homebrew, or direct download. Query-side MVP first (no LLM integration needed).

## Architecture Overview

```
reasons (binary)
├── main.rs              — CLI entry point (clap)
├── db.rs                — SQLite schema, migrations, CRUD
├── tms.rs               — Truth maintenance engine (propagation, justifications)
├── commands/
│   ├── mod.rs
│   ├── add.rs           — add, add-justification, remove-justification
│   ├── retract.rs       — retract, assert
│   ├── query.rs         — show, explain, search, lookup, list
│   ├── export.rs        — export (JSON), export-markdown, import-beliefs, import-json
│   ├── tree.rs          — tree visualization (NEW — not in Python version)
│   ├── nogood.rs        — nogood, find-culprits
│   ├── challenge.rs     — challenge, defend, supersede
│   ├── manage.rs        — init, status, propagate, log, update, set-metadata, get-metadata
│   └── mcp.rs           — MCP server mode (Phase 3)
└── format.rs            — Output formatting (markdown, JSON, minimal)
```

### Dependencies

- `clap` — CLI argument parsing (derive API)
- `rusqlite` — SQLite with bundled feature (includes FTS5)
- `serde` / `serde_json` — JSON serialization
- `chrono` — ISO 8601 timestamps

## Phase 1: Core Engine + Query Commands

This phase produces a working binary that can open any existing `reasons.db` and query it. No write operations yet.

### 1.1 SQLite Schema and Database Layer (`db.rs`)

Implement the database layer that can open and read an existing `reasons.db`.

**Tables (read from existing DB, create on `init`):**

```sql
CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    text TEXT NOT NULL,
    truth_value TEXT NOT NULL DEFAULT 'IN',
    source TEXT DEFAULT '',
    source_url TEXT DEFAULT '',
    source_hash TEXT DEFAULT '',
    date TEXT DEFAULT '',
    metadata_json TEXT DEFAULT '{}',
    created_at TEXT DEFAULT '',
    updated_at TEXT DEFAULT '',
    reviewed_at TEXT DEFAULT '',
    verified_at TEXT DEFAULT '',
    retracted_at TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS justifications (
    rowid INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id TEXT NOT NULL REFERENCES nodes(id),
    type TEXT NOT NULL,
    antecedents_json TEXT NOT NULL DEFAULT '[]',
    outlist_json TEXT NOT NULL DEFAULT '[]',
    label TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS nogoods (
    id TEXT PRIMARY KEY,
    nodes_json TEXT NOT NULL DEFAULT '[]',
    discovered TEXT DEFAULT '',
    resolution TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS repos (
    name TEXT PRIMARY KEY,
    path TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS propagation_log (
    rowid INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS network_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
    id, text, tokenize="porter unicode61"
);
```

**Pragmas:** `journal_mode=WAL`, `foreign_keys=ON`

**Schema migrations:** On open, check for missing columns (`source_url`, timestamp columns) and `ALTER TABLE ADD COLUMN` if absent.

**Structs:**

```rust
struct Node {
    id: String,
    text: String,
    truth_value: String,        // "IN" or "OUT"
    source: String,
    source_url: String,
    source_hash: String,
    date: String,
    metadata: serde_json::Value, // JSON object
    created_at: String,
    updated_at: String,
    reviewed_at: String,
    verified_at: String,
    retracted_at: String,
}

struct Justification {
    rowid: i64,
    node_id: String,
    jtype: String,              // "SL" or "CP"
    antecedents: Vec<String>,   // stored as JSON array in DB
    outlist: Vec<String>,       // stored as JSON array in DB
    label: String,
}

struct Nogood {
    id: String,
    nodes: Vec<String>,         // stored as JSON array in DB
    discovered: String,
    resolution: String,
}
```

**Functions:**
- `open_db(path) -> Connection` — open with pragmas, run migrations
- `init_db(path) -> Connection` — create new database with full schema
- `load_node(conn, id) -> Option<Node>`
- `load_all_nodes(conn) -> Vec<Node>`
- `load_justifications(conn, node_id) -> Vec<Justification>`
- `load_all_justifications(conn) -> Vec<Justification>`
- `load_nogoods(conn) -> Vec<Nogood>`
- `load_meta(conn, key) -> Option<String>`

### 1.2 Query Commands (`commands/query.rs`)

#### `reasons show <NODE_ID>`

Load node + its justifications + its dependents. Display:

```
node-id [IN]
  Text: The belief text here.
  Source: source_document
  Source URL: https://...
  Created: 2026-07-06T12:00:00
  
  Justifications:
    [0] SL: antecedent-1, antecedent-2
    [1] SL: antecedent-3 (unless: outlist-1)
  
  Dependents:
    dependent-1 [IN]: Dependent belief text...
    dependent-2 [OUT]: Another dependent...
```

Dependents are found by querying justifications where `antecedents_json` or `outlist_json` contains the node ID. This requires scanning all justifications — build a reverse index in memory.

#### `reasons explain <NODE_ID>`

Trace why a node is IN or OUT. Recursive walk through justification chains.

For IN nodes: find the valid justification, show its antecedents, recurse.
For OUT nodes: show each justification and which antecedent is OUT or which outlist item is IN.

Output format:
```
node-id is IN because:
  Justified by SL: antecedent-1, antecedent-2
    antecedent-1 is IN (premise)
    antecedent-2 is IN because:
      Justified by SL: deeper-node
        deeper-node is IN (premise)
```

**Algorithm:**
```
explain(node_id, visited, depth):
    if node_id in visited: print "(circular)" and return
    visited.add(node_id)
    node = load_node(node_id)
    justifications = load_justifications(node_id)
    
    if justifications.is_empty():
        print "{node_id} is {truth_value} (premise)"
        return
    
    if node.truth_value == "IN":
        find first valid justification
        print "{node_id} is IN because: Justified by {type}: {antecedents}"
        for ant_id in justification.antecedents:
            explain(ant_id, visited, depth+1)  // recurse
    else:
        print "{node_id} is OUT because:"
        for each justification:
            explain which conditions fail
```

#### `reasons search <QUERY> [--format markdown|json|minimal] [--depth N]`

1. Extract words from query, filter stop words (hardcode the 117-word English stop list)
2. Build FTS5 query: quote each term, join with spaces
3. `SELECT id FROM nodes_fts WHERE nodes_fts MATCH ? ORDER BY rank LIMIT 20`
4. If no results and 2+ terms: progressive relaxation (try subsets of terms)
5. Fallback: `SELECT id FROM nodes WHERE text LIKE '%query%' LIMIT 20`
6. Expand neighbors via BFS to `--depth` (default 1): add antecedents and dependents of matched nodes
7. Format output per `--format` flag

**Stop words list:** a, about, above, after, again, against, all, am, an, and, any, are, aren't, as, at, be, because, been, before, being, below, between, both, but, by, can't, cannot, could, couldn't, did, didn't, do, does, doesn't, doing, don't, down, during, each, few, for, from, further, get, got, had, hadn't, has, hasn't, have, haven't, having, he, her, here, hers, herself, him, himself, his, how, i, if, in, into, is, isn't, it, its, itself, just, let, like, ll, me, might, more, most, mustn't, my, myself, no, nor, not, now, of, off, on, once, only, or, other, our, ours, ourselves, out, over, own, re, s, same, shan't, she, should, shouldn't, so, some, such, t, than, that, the, their, theirs, them, themselves, then, there, these, they, this, those, through, to, too, under, until, up, ve, very, was, wasn't, we, were, weren't, what, when, where, which, while, who, whom, why, will, with, won't, would, wouldn't, you, your, yours, yourself, yourselves

#### `reasons lookup <QUERY>`

Simple substring search: `SELECT id, text, truth_value FROM nodes WHERE text LIKE '%query%' OR id LIKE '%query%'`

No neighbor expansion. Compact output.

#### `reasons list [--status IN|OUT] [--premises] [--has-dependents] [--min-depth N] [--max-depth N] [--by-impact]`

Build a `SELECT` query with optional `WHERE` clauses:
- `--status IN|OUT`: filter by `truth_value`
- `--premises`: nodes with zero justifications
- `--has-dependents`: nodes that appear in some justification's antecedents/outlist
- `--min-depth` / `--max-depth`: filter by derivation depth (count longest justification chain to a premise)
- `--by-impact`: sort by number of dependents (descending)

Output: one line per node: `[IN/OUT] node-id: First 80 chars of text...`

### 1.3 Tree Visualization (`commands/tree.rs`)

**NEW — does not exist in the Python version.** This is the highest-leverage UX addition.

#### `reasons tree <NODE_ID> [--direction up|down|both] [--depth N]`

- `--direction up` (default): show what this node depends on (antecedents)
- `--direction down`: show what depends on this node (dependents)
- `--direction both`: show both
- `--depth`: max depth to traverse (default: unlimited)

Output format (tree-style):
```
node-id [IN]: The belief text
├── antecedent-1 [IN]: Text of antecedent 1
│   ├── deeper-1 [IN]: Premise text (premise)
│   └── deeper-2 [OUT]: Another premise (premise)
└── antecedent-2 [IN]: Text of antecedent 2
    └── deeper-3 [IN]: Yet another (premise)
```

Use Unicode box-drawing characters. Mark premises. Show `[IN]`/`[OUT]` status. Truncate long text.

**Algorithm:**
```
tree(node_id, direction, max_depth):
    node = load_node(node_id)
    print root line
    if direction == up or both:
        justifications = load_justifications(node_id)
        for each justification:
            for ant_id in antecedents:
                print_subtree(ant_id, "up", depth=1, max_depth, visited={node_id})
    if direction == down or both:
        dependents = find_dependents(node_id)
        for dep_id in dependents:
            print_subtree(dep_id, "down", depth=1, max_depth, visited={node_id})
```

### 1.4 Export Commands (`commands/export.rs`)

#### `reasons export [--output FILE]`

Export full network as JSON. Schema:

```json
{
  "meta": {
    "schema_version": "1.0",
    "project_name": "...",
    "created_at": "...",
    "updated_at": "...",
    "node_count": 42,
    "generator": "reasons/0.1.0"
  },
  "nodes": { "node-id": { ... } },
  "nogoods": [ { ... } ],
  "repos": { "name": "/path" }
}
```

Exclude metadata keys starting with `_` from export.

#### `reasons export-markdown [--output FILE]`

Export as `beliefs.md` format with YAML frontmatter. Must be round-trip compatible with `import-beliefs`.

```markdown
---
schema_version: "1.0"
project_name: "..."
updated_at: "..."
node_count: 42
generator: "reasons/0.1.0"
---

# Belief Registry

## Claims

### node-id [IN] OBSERVATION
Belief text here.

- Source: source_document
- Depends on: dep1, dep2
```

Status mapping: `truth_value="OUT"` and (`stale_reason` or `retract_reason` in metadata) → `STALE`, else truth_value.
Type: `beliefs_type` from metadata, or `DERIVED` if has justifications, else `OBSERVATION`.

### 1.5 Init and Status (`commands/manage.rs`)

#### `reasons init`

Create a new `reasons.db` in the current directory with full schema. Set `network_meta` values: `schema_version=1.0`, `created_at=now()`, `updated_at=now()`.

#### `reasons status`

Show summary:
```
reasons.db
  Nodes: 42 (38 IN, 4 OUT)
  Premises: 25
  Derived: 17
  Nogoods: 2
  Last updated: 2026-07-06T12:00:00
```

#### `reasons log [--limit N]`

Show propagation log, most recent first. Default limit 50.

```
2026-07-06T12:00:00  retract  node-id  OUT
2026-07-06T12:00:00  propagate  dependent-1  OUT
```

### 1.6 CLI Entry Point (`main.rs`)

Use `clap` derive API. Top-level structure:

```rust
#[derive(Parser)]
#[command(name = "reasons", about = "Truth Maintenance System")]
struct Cli {
    #[arg(long, default_value = "reasons.db")]
    db: PathBuf,
    
    #[command(subcommand)]
    command: Commands,
}

enum Commands {
    Init,
    Status,
    Show { node_id: String },
    Explain { node_id: String },
    Search { query: String, #[arg(long, default_value = "markdown")] format: String, #[arg(long, default_value_t = 1)] depth: usize },
    Lookup { query: String },
    List { #[arg(long)] status: Option<String>, #[arg(long)] premises: bool, ... },
    Tree { node_id: String, #[arg(long, default_value = "up")] direction: String, #[arg(long)] depth: Option<usize> },
    Export { #[arg(long)] output: Option<PathBuf> },
    ExportMarkdown { #[arg(long)] output: Option<PathBuf> },
    Log { #[arg(long, default_value_t = 50)] limit: usize },
    // Phase 2 commands added later
}
```

The `--db` flag defaults to `reasons.db` in the current directory. All commands accept it.

---

## Phase 2: Write Operations + TMS Engine

This phase adds the ability to modify the belief network.

### 2.1 TMS Engine (`tms.rs`)

Core truth maintenance logic. Operates on in-memory structs loaded from DB, writes results back.

**Key types:**

```rust
struct Network {
    nodes: HashMap<String, Node>,
    justifications: HashMap<String, Vec<Justification>>,  // node_id -> justifications
    dependents: HashMap<String, HashSet<String>>,          // node_id -> set of dependent node IDs
}
```

**Build dependent index on load:**
```
for each justification:
    for ant_id in antecedents:
        dependents[ant_id].insert(justification.node_id)
    for out_id in outlist:
        dependents[out_id].insert(justification.node_id)
```

**Core functions:**

- `compute_truth(network, node_id) -> &str` — check all justifications; return "IN" if any valid, "OUT" otherwise. Premises (no justifications) keep their current value.

- `justification_valid(network, j) -> bool` — all antecedents must be IN, all outlist must be OUT (or absent).

- `propagate(network, changed_id) -> Vec<String>` — BFS from changed node through dependents. For each dependent: recompute truth, if changed add to queue. Skip retracted nodes. Return list of all changed IDs.

- `retract(network, node_id, reason) -> Vec<String>` — set OUT, set `_retracted=true`, set `retracted_at`, propagate. Return all changed.

- `assert_node(network, node_id) -> Vec<String>` — set IN, clear `_retracted`, propagate. Return all changed.

- `add_justification(network, node_id, justification) -> Vec<String>` — append justification, update dependent index, recompute truth, propagate if changed. Inherit access_tags from antecedents.

- `remove_justification(network, node_id, index) -> Vec<String>` — remove by index, clean up dependent refs (only if no other justification references them), recompute, propagate.

- `recompute_all(network) -> Vec<String>` — iterate to fixpoint. Max iterations = node count + 1. Each pass: recompute all non-premise non-retracted nodes. Stop when no changes.

### 2.2 Write Commands (`commands/add.rs`, `commands/retract.rs`)

#### `reasons add <NODE_ID> <TEXT> [--sl DEP1,DEP2] [--unless OUT1,OUT2] [--source S] [--source-url U]`

- If `--sl` provided: create with SL justification from listed antecedents
- If `--unless` provided: add outlist to the justification
- If no `--sl`: create as premise (no justifications)
- Compute initial truth value, propagate, log
- Write to DB: insert into `nodes`, `justifications`, update `nodes_fts`

#### `reasons add-justification <NODE_ID> --sl DEP1,DEP2 [--unless OUT1,OUT2]`

Add new justification to existing node. Recompute and propagate.

#### `reasons remove-justification <NODE_ID> --index N`

Remove justification by index. Recompute and propagate.

#### `reasons retract <NODE_ID> [--reason TEXT]`

Retract node. Set OUT, metadata `_retracted=true`, `retract_reason`. Propagate cascade.

Print all affected nodes:
```
Retracted node-id
  Cascaded: dependent-1 OUT, dependent-2 OUT
```

#### `reasons assert <NODE_ID>`

Re-assert a retracted node. Clear `_retracted`, set IN. Propagate.

#### `reasons update <NODE_ID> [--text T] [--source S] [--source-url U]`

Modify node fields in place. Set `updated_at`.

#### `reasons set-metadata <NODE_ID> <KEY> <VALUE>`

Set a metadata key on a node. Value is parsed as JSON if possible, otherwise stored as string.

#### `reasons get-metadata <NODE_ID> [KEY]`

Print metadata. If key given, print just that value. Otherwise print all metadata as JSON.

### 2.3 Challenge, Defense, Supersede (`commands/challenge.rs`)

#### `reasons challenge <NODE_ID> --reason TEXT [--challenge-id ID]`

- Create new challenge node (premise, IN)
- Add challenge ID to target's outlist (converts premise to justified if needed)
- Propagate — target likely goes OUT
- Set metadata: `challenge_target` on challenge, `challenges` list on target

#### `reasons defend <NODE_ID> --challenge-id CHALLENGE_ID --reason TEXT [--defense-id ID]`

- Create defense node
- Add challenge to defense's outlist
- When defense IN → challenge OUT → target restored
- Set metadata: `defense_target`, `defends`

#### `reasons supersede --old-id OLD --new-id NEW`

- Add new to old's outlist
- When new is IN, old goes OUT
- Reversible: retract new → old restores
- Metadata: `superseded_by` on old, `supersedes` on new

### 2.4 Contradiction Handling (`commands/nogood.rs`)

#### `reasons nogood <NODE_ID> [NODE_ID ...]`

- Record contradiction in nogoods table
- Check if all listed nodes are currently IN
- If yes: run dependency-directed backtracking
  - Trace each nogood node backward to premises
  - Score each premise by entrenchment:
    - +100 if premise (no justifications)
    - +50 if has source
    - +25 if has source_hash
    - +10 per dependent
    - +type_score (AXIOM/WARNING: 90, OBSERVATION: 80, DERIVED: 40, PREDICTED: 30, NOTE: 10)
  - Retract the lowest-scored premise
- Print resolution

#### `reasons find-culprits <NODE_ID> [NODE_ID ...]`

Same trace + scoring but don't retract — just print candidates ranked by entrenchment.

### 2.5 Import Commands (`commands/export.rs`)

#### `reasons import-beliefs <FILE>`

Parse `beliefs.md` format. For each `### node-id [STATUS] TYPE` section:
- Extract text, source, depends-on, unless
- Create node with appropriate justifications
- Recompute truth values after all imports

#### `reasons import-json <FILE>`

Parse JSON export format. Load nodes, justifications, nogoods. Recompute.

### 2.6 Additional Management Commands

#### `reasons propagate`

Force full recompute to fixpoint. Print all changed nodes.

#### `reasons trace <NODE_ID>`

Walk backward through justification chains, collect all premise IDs.

```
Premises supporting node-id:
  premise-1: Text of premise 1
  premise-2: Text of premise 2
```

#### `reasons convert-to-premise <NODE_ID>`

Strip all justifications from a node, making it a premise.

---

## Phase 3: MCP Server Mode

#### `reasons mcp [--db PATH]`

Run as MCP server over stdio transport. This is the primary interface for Claude Desktop users.

**Claude Desktop configuration:**
```json
{
  "mcpServers": {
    "reasons": {
      "command": "reasons",
      "args": ["mcp", "--db", "/path/to/reasons.db"]
    }
  }
}
```

**Exposed tools:**

| Tool | Description | Phase |
|------|-------------|-------|
| `search` | FTS5 search with neighbor expansion | Query |
| `show` | Show node details | Query |
| `explain` | Trace why IN/OUT | Query |
| `tree` | Dependency tree visualization | Query |
| `list` | Filter and list nodes | Query |
| `add` | Add a belief | Write |
| `retract` | Retract with cascade | Write |
| `assert` | Re-assert a retracted belief | Write |
| `challenge` | Challenge a belief | Write |
| `defend` | Defend against a challenge | Write |
| `nogood` | Record contradiction | Write |

**Implementation:** Use the `rmcp` crate (Rust MCP SDK) for stdio transport. Each tool maps directly to the corresponding CLI command logic.

---

## Testing Strategy

### Compatibility Tests

The most important tests: create a `reasons.db` with the Python ftl-reasons, then read and query it with the Rust binary. This validates schema compatibility and query correctness.

1. Use a known `reasons.db` fixture (copy one from an existing project)
2. Run every query command against it
3. Compare output to Python version's output
4. For write operations: write with Rust, read with Python, verify round-trip

### Unit Tests

- `tms.rs`: test propagation, justification validity, retraction cascades, fixpoint convergence, entrenchment scoring
- `db.rs`: test schema creation, migrations, CRUD operations
- `format.rs`: test output formatting for all three formats
- `commands/tree.rs`: test tree rendering with cycles, deep chains, wide trees

### Integration Tests

- `reasons init && reasons add foo "test" && reasons show foo` — full lifecycle
- Import a `beliefs.md`, export it, diff — round-trip fidelity
- Retract a premise, verify cascade, assert it back, verify restoration
- Nogood with backtracking — verify correct culprit selection

---

## Build and Release

### Cargo.toml

```toml
[package]
name = "reasons"
version = "0.1.0"
edition = "2021"
description = "Truth Maintenance System for managing justified beliefs"
license = "MIT"

[dependencies]
clap = { version = "4", features = ["derive"] }
rusqlite = { version = "0.31", features = ["bundled", "vtab"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"

[profile.release]
lto = true
strip = true
```

The `bundled` feature for rusqlite statically links SQLite including FTS5 — no system SQLite dependency.

### Cross-Platform Builds (GitHub Actions)

Build matrix:
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Upload binaries to GitHub Releases on tag push.

### Homebrew

Create a `homebrew-tap` repo with a formula that downloads the prebuilt binary for the user's platform.

---

## Implementation Order

An implementing agent should follow this order, completing and testing each step before moving to the next:

1. **Scaffold** — `cargo init`, add dependencies to `Cargo.toml`, set up `src/` structure with empty modules
2. **`db.rs`** — schema creation, open with migrations, load functions. Test: create a DB, load a known fixture.
3. **`reasons init`** — first working command. Test: creates a valid `reasons.db`.
4. **`reasons status`** — read-only summary. Test against a fixture DB.
5. **`reasons show`** — load node + justifications + dependents. Test: matches Python output.
6. **`reasons explain`** — recursive justification trace. Test: IN and OUT cases, circular deps.
7. **`reasons search`** — FTS5 with stop words, relaxation, neighbor expansion. Test: known queries against fixture.
8. **`reasons lookup`** — substring search. Simple.
9. **`reasons list`** — filtered listing with all flag combinations.
10. **`reasons tree`** — tree visualization. Test: up/down/both, depth limits, cycles.
11. **`reasons export`** — JSON export. Test: round-trip with Python's import.
12. **`reasons export-markdown`** — beliefs.md export. Test: round-trip.
13. **`tms.rs`** — propagation engine, justification validity, retraction, assertion. Unit test heavily.
14. **`reasons add`** — first write command. Test: add + show round-trip.
15. **`reasons add-justification`** / **`reasons remove-justification`**
16. **`reasons retract`** / **`reasons assert`** — with cascade verification.
17. **`reasons import-beliefs`** / **`reasons import-json`** — parse and load.
18. **`reasons update`** / **`reasons set-metadata`** / **`reasons get-metadata`**
19. **`reasons challenge`** / **`reasons defend`** / **`reasons supersede`**
20. **`reasons nogood`** / **`reasons find-culprits`** — with entrenchment scoring and backtracking.
21. **`reasons propagate`** — force full recompute.
22. **`reasons trace`** / **`reasons convert-to-premise`**
23. **`reasons log`** — propagation log display.
24. **`reasons mcp`** — MCP server mode via rmcp.
25. **GitHub Actions release pipeline** — cross-platform builds on tag push.

---

## Compatibility Notes

- The Rust binary must read and write the same `reasons.db` schema as the Python version
- `beliefs.md` export format must be parseable by the Python `import-beliefs` command and vice versa
- JSON export format must match the Python schema exactly (field names, nesting, types)
- FTS5 tokenizer must match: `porter unicode61`
- The `--db` flag should accept both relative and absolute paths
- Default database name is `reasons.db` in the current directory, matching the Python behavior

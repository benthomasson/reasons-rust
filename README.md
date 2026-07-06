# reasons

A Truth Maintenance System (TMS) for managing justified beliefs with dependency tracking, contradiction detection, and truth-value propagation.

Rust port of [ftl-reasons](https://github.com/benthomasson/ftl-reasons). Single static binary, zero runtime dependencies.

## Install

### Homebrew

```bash
brew tap benthomasson/tap
brew install reasons
```

### From source

```bash
cargo install --path .
```

### From GitHub Releases

Download a prebuilt binary from [Releases](https://github.com/benthomasson/reasons-rust/releases) for your platform.

## Quick start

```bash
# Initialize a database
reasons init

# Add some beliefs
reasons add climate-change "Global temperatures are rising" --source "NASA"
reasons add ice-melting "Arctic ice is melting" --sl climate-change
reasons add sea-level "Sea levels are rising" --sl ice-melting

# Inspect the network
reasons show climate-change
reasons tree sea-level --direction up
reasons explain sea-level

# Retract a belief and watch the cascade
reasons retract climate-change --reason "Hypothetical exercise"
reasons list --status OUT

# Restore it
reasons assert climate-change
```

## Core concepts

**Nodes** are beliefs with a truth value (IN or OUT) and optional metadata (source, URL, timestamps).

**Justifications** link nodes via support lists (antecedents that must be IN) and outlists (nodes that must be OUT). A node with at least one valid justification is IN; otherwise it's OUT. Nodes without justifications are **premises** that keep their truth value directly.

**Propagation** cascades truth-value changes through the dependency graph using BFS. Retracting a premise automatically flips all dependent nodes to OUT.

**Challenges** create a new node added to the target's outlist, flipping it OUT. **Defenses** counter-challenge the challenge, restoring the original.

**Nogoods** record contradictions. When all nodes in a nogood are IN, dependency-directed backtracking retracts the least-entrenched premise.

## Commands

### Query

| Command | Description |
|---------|-------------|
| `show <ID>` | Node details, justifications, and dependents |
| `explain <ID>` | Recursive trace of why a node is IN or OUT |
| `search <QUERY>` | Full-text search with progressive relaxation and neighbor expansion |
| `lookup <QUERY>` | Simple substring search |
| `list` | List nodes with `--status`, `--premises`, `--has-dependents`, `--by-impact` filters |
| `tree <ID>` | Dependency tree with `--direction up\|down\|both` and `--depth` |

### Write

| Command | Description |
|---------|-------------|
| `add <ID> <TEXT>` | Add a premise; use `--sl a,b` for derived nodes |
| `add-justification <ID> --sl a,b` | Add justification to existing node |
| `remove-justification <ID> --index N` | Remove justification by index |
| `retract <ID>` | Mark OUT with cascade propagation |
| `assert <ID>` | Restore a retracted node to IN |
| `update <ID>` | Modify text, source, or source URL |
| `challenge <ID> --reason TEXT` | Challenge a belief |
| `defend <ID> --challenge-id CID --reason TEXT` | Defend against a challenge |
| `supersede --old-id OLD --new-id NEW` | Mark a belief as superseded |
| `nogood <ID> [ID ...]` | Record contradiction with auto-backtracking |

### Management

| Command | Description |
|---------|-------------|
| `init` | Create a new `reasons.db` |
| `status` | Database summary |
| `propagate` | Force full truth-value recomputation |
| `trace <ID>` | Find all supporting premises |
| `convert-to-premise <ID>` | Strip justifications from a derived node |
| `export [-o FILE]` | Export as JSON |
| `export-markdown [-o FILE]` | Export as markdown |
| `import-json <FILE>` | Import from JSON |
| `import-beliefs <FILE>` | Import from markdown |
| `log [--limit N]` | Show propagation log |

## MCP server

Run as an [MCP](https://modelcontextprotocol.io) server over stdio for use with Claude Desktop or Claude Code:

```bash
reasons mcp --db /path/to/reasons.db
```

Claude Desktop configuration (`claude_desktop_config.json`):

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

Exposes 11 tools: `search`, `show`, `explain`, `tree`, `list`, `add`, `retract`, `assert_node`, `challenge`, `defend`, `nogood`.

## Database

All data is stored in a single `reasons.db` SQLite file (default: current directory, override with `--db`). The schema is compatible with the Python [ftl-reasons](https://github.com/benthomasson/ftl-reasons) version.

## License

MIT

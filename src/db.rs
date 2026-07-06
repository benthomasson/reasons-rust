use rusqlite::{Connection, Result, params};
use serde_json;
use std::path::Path;

use crate::types::{Node, Justification, Nogood};

const SCHEMA_SQL: &str = "
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
";

const FTS_SQL: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
    id, text, tokenize=\"porter unicode61\"
);
";

fn set_pragmas(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;
    ")
}

fn run_migrations(conn: &Connection) -> Result<()> {
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(nodes)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>>>()?;

    let migrations = [
        ("source_url", "ALTER TABLE nodes ADD COLUMN source_url TEXT DEFAULT ''"),
        ("source_hash", "ALTER TABLE nodes ADD COLUMN source_hash TEXT DEFAULT ''"),
        ("created_at", "ALTER TABLE nodes ADD COLUMN created_at TEXT DEFAULT ''"),
        ("updated_at", "ALTER TABLE nodes ADD COLUMN updated_at TEXT DEFAULT ''"),
        ("reviewed_at", "ALTER TABLE nodes ADD COLUMN reviewed_at TEXT DEFAULT ''"),
        ("verified_at", "ALTER TABLE nodes ADD COLUMN verified_at TEXT DEFAULT ''"),
        ("retracted_at", "ALTER TABLE nodes ADD COLUMN retracted_at TEXT DEFAULT ''"),
    ];

    for (col, sql) in &migrations {
        if !columns.iter().any(|c| c == col) {
            conn.execute(sql, [])?;
        }
    }

    let just_columns: Vec<String> = conn
        .prepare("PRAGMA table_info(justifications)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>>>()?;

    if !just_columns.iter().any(|c| c == "label") {
        conn.execute("ALTER TABLE justifications ADD COLUMN label TEXT DEFAULT ''", [])?;
    }

    Ok(())
}

pub fn open_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    set_pragmas(&conn)?;
    run_migrations(&conn)?;
    Ok(conn)
}

pub fn init_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    set_pragmas(&conn)?;
    conn.execute_batch(SCHEMA_SQL)?;
    conn.execute_batch(FTS_SQL)?;

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR IGNORE INTO network_meta (key, value) VALUES (?1, ?2)",
        params!["schema_version", "1.0"],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO network_meta (key, value) VALUES (?1, ?2)",
        params!["created_at", &now],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO network_meta (key, value) VALUES (?1, ?2)",
        params!["updated_at", &now],
    )?;

    Ok(conn)
}

pub fn load_node(conn: &Connection, id: &str) -> Result<Option<Node>> {
    let mut stmt = conn.prepare(
        "SELECT id, text, truth_value, source, source_url, source_hash, date,
                metadata_json, created_at, updated_at, reviewed_at, verified_at, retracted_at
         FROM nodes WHERE id = ?1"
    )?;

    let mut rows = stmt.query_map(params![id], row_to_node)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn load_all_nodes(conn: &Connection) -> Result<Vec<Node>> {
    let mut stmt = conn.prepare(
        "SELECT id, text, truth_value, source, source_url, source_hash, date,
                metadata_json, created_at, updated_at, reviewed_at, verified_at, retracted_at
         FROM nodes"
    )?;
    let rows = stmt.query_map([], row_to_node)?;
    rows.collect()
}

fn row_to_node(row: &rusqlite::Row) -> Result<Node> {
    let metadata_str: String = row.get(7)?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata_str)
        .unwrap_or_else(|_| serde_json::json!({}));

    Ok(Node {
        id: row.get(0)?,
        text: row.get(1)?,
        truth_value: row.get(2)?,
        source: row.get(3)?,
        source_url: row.get(4)?,
        source_hash: row.get(5)?,
        date: row.get(6)?,
        metadata,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        reviewed_at: row.get(10)?,
        verified_at: row.get(11)?,
        retracted_at: row.get(12)?,
    })
}

pub fn load_justifications(conn: &Connection, node_id: &str) -> Result<Vec<Justification>> {
    let mut stmt = conn.prepare(
        "SELECT rowid, node_id, type, antecedents_json, outlist_json, label
         FROM justifications WHERE node_id = ?1"
    )?;
    let rows = stmt.query_map(params![node_id], row_to_justification)?;
    rows.collect()
}

pub fn load_all_justifications(conn: &Connection) -> Result<Vec<Justification>> {
    let mut stmt = conn.prepare(
        "SELECT rowid, node_id, type, antecedents_json, outlist_json, label
         FROM justifications"
    )?;
    let rows = stmt.query_map([], row_to_justification)?;
    rows.collect()
}

fn row_to_justification(row: &rusqlite::Row) -> Result<Justification> {
    let ant_str: String = row.get(3)?;
    let out_str: String = row.get(4)?;

    let antecedents: Vec<String> = serde_json::from_str(&ant_str).unwrap_or_default();
    let outlist: Vec<String> = serde_json::from_str(&out_str).unwrap_or_default();

    Ok(Justification {
        rowid: row.get(0)?,
        node_id: row.get(1)?,
        jtype: row.get(2)?,
        antecedents,
        outlist,
        label: row.get(5)?,
    })
}

pub fn load_nogoods(conn: &Connection) -> Result<Vec<Nogood>> {
    let mut stmt = conn.prepare(
        "SELECT id, nodes_json, discovered, resolution FROM nogoods"
    )?;
    let rows = stmt.query_map([], |row| {
        let nodes_str: String = row.get(1)?;
        let nodes: Vec<String> = serde_json::from_str(&nodes_str).unwrap_or_default();
        Ok(Nogood {
            id: row.get(0)?,
            nodes,
            discovered: row.get(2)?,
            resolution: row.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn load_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM network_meta WHERE key = ?1")?;
    let mut rows = stmt.query_map(params![key], |row| row.get(0))?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn load_repos(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT name, path FROM repos")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut map = std::collections::HashMap::new();
    for r in rows {
        let (k, v) = r?;
        map.insert(k, v);
    }
    Ok(map)
}

pub fn save_node(conn: &Connection, node: &Node) -> Result<()> {
    let metadata_str = serde_json::to_string(&node.metadata).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT OR REPLACE INTO nodes (id, text, truth_value, source, source_url, source_hash,
         date, metadata_json, created_at, updated_at, reviewed_at, verified_at, retracted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            node.id, node.text, node.truth_value, node.source, node.source_url,
            node.source_hash, node.date, metadata_str, node.created_at, node.updated_at,
            node.reviewed_at, node.verified_at, node.retracted_at
        ],
    )?;
    Ok(())
}

pub fn save_justification(conn: &Connection, j: &Justification) -> Result<i64> {
    let ant_str = serde_json::to_string(&j.antecedents).unwrap_or_else(|_| "[]".to_string());
    let out_str = serde_json::to_string(&j.outlist).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO justifications (node_id, type, antecedents_json, outlist_json, label)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![j.node_id, j.jtype, ant_str, out_str, j.label],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_justification(conn: &Connection, rowid: i64) -> Result<()> {
    conn.execute("DELETE FROM justifications WHERE rowid = ?1", params![rowid])?;
    Ok(())
}

pub fn update_node_truth(conn: &Connection, id: &str, truth_value: &str) -> Result<()> {
    conn.execute(
        "UPDATE nodes SET truth_value = ?1 WHERE id = ?2",
        params![truth_value, id],
    )?;
    Ok(())
}

pub fn update_node_metadata(conn: &Connection, id: &str, metadata: &serde_json::Value) -> Result<()> {
    let metadata_str = serde_json::to_string(metadata).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "UPDATE nodes SET metadata_json = ?1 WHERE id = ?2",
        params![metadata_str, id],
    )?;
    Ok(())
}

pub fn update_node_field(conn: &Connection, id: &str, field: &str, value: &str) -> Result<()> {
    let sql = format!("UPDATE nodes SET {} = ?1 WHERE id = ?2", field);
    conn.execute(&sql, params![value, id])?;
    Ok(())
}

pub fn save_nogood(conn: &Connection, nogood: &Nogood) -> Result<()> {
    let nodes_str = serde_json::to_string(&nogood.nodes).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT OR REPLACE INTO nogoods (id, nodes_json, discovered, resolution)
         VALUES (?1, ?2, ?3, ?4)",
        params![nogood.id, nodes_str, nogood.discovered, nogood.resolution],
    )?;
    Ok(())
}

pub fn log_propagation(conn: &Connection, action: &str, target: &str, value: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO propagation_log (timestamp, action, target, value) VALUES (?1, ?2, ?3, ?4)",
        params![now, action, target, value],
    )?;
    Ok(())
}

pub fn load_propagation_log(conn: &Connection, limit: usize) -> Result<Vec<(String, String, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT timestamp, action, target, value FROM propagation_log ORDER BY rowid DESC LIMIT ?1"
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })?;
    rows.collect()
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO network_meta (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

pub fn rebuild_fts(conn: &Connection) -> Result<()> {
    conn.execute("DROP TABLE IF EXISTS nodes_fts", [])?;
    conn.execute_batch(FTS_SQL)?;
    conn.execute(
        "INSERT INTO nodes_fts (id, text) SELECT id, text FROM nodes",
        [],
    )?;
    Ok(())
}

pub fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM nodes_fts WHERE nodes_fts MATCH ?1 ORDER BY rank LIMIT ?2"
    )?;
    let rows = stmt.query_map(params![query, limit as i64], |row| row.get(0))?;
    rows.collect()
}

pub fn substring_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<String>> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id FROM nodes WHERE text LIKE ?1 OR id LIKE ?1 LIMIT ?2"
    )?;
    let rows = stmt.query_map(params![pattern, limit as i64], |row| row.get(0))?;
    rows.collect()
}

pub fn node_count(conn: &Connection) -> Result<(usize, usize)> {
    let total: usize = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
    let in_count: usize = conn.query_row(
        "SELECT COUNT(*) FROM nodes WHERE truth_value = 'IN'", [], |r| r.get(0)
    )?;
    Ok((total, in_count))
}

pub fn justification_count_for_node(conn: &Connection, node_id: &str) -> Result<usize> {
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM justifications WHERE node_id = ?1",
        params![node_id],
        |r| r.get(0),
    )?;
    Ok(count)
}

pub fn nogood_count(conn: &Connection) -> Result<usize> {
    let count: usize = conn.query_row("SELECT COUNT(*) FROM nogoods", [], |r| r.get(0))?;
    Ok(count)
}

pub fn delete_node(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM justifications WHERE node_id = ?1", params![id])?;
    conn.execute("DELETE FROM nodes WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Node, Justification, Nogood};

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        set_pragmas(&conn).unwrap();
        conn.execute_batch(SCHEMA_SQL).unwrap();
        conn.execute_batch(FTS_SQL).unwrap();
        conn
    }

    fn sample_node(id: &str, text: &str) -> Node {
        Node::new(id.to_string(), text.to_string())
    }

    #[test]
    fn empty_db_counts() {
        let conn = test_db();
        assert_eq!(node_count(&conn).unwrap(), (0, 0));
        assert_eq!(nogood_count(&conn).unwrap(), 0);
        assert!(load_all_nodes(&conn).unwrap().is_empty());
    }

    #[test]
    fn save_and_load_node_round_trip() {
        let conn = test_db();
        let mut node = sample_node("test-1", "Test belief");
        node.source = "doc.md".to_string();
        node.source_url = "https://example.com".to_string();
        node.source_hash = "abc123".to_string();

        save_node(&conn, &node).unwrap();
        let loaded = load_node(&conn, "test-1").unwrap().unwrap();

        assert_eq!(loaded.id, "test-1");
        assert_eq!(loaded.text, "Test belief");
        assert_eq!(loaded.truth_value, "IN");
        assert_eq!(loaded.source, "doc.md");
        assert_eq!(loaded.source_url, "https://example.com");
        assert_eq!(loaded.source_hash, "abc123");
    }

    #[test]
    fn load_nonexistent_node_returns_none() {
        let conn = test_db();
        assert!(load_node(&conn, "nope").unwrap().is_none());
    }

    #[test]
    fn save_and_load_justification_round_trip() {
        let conn = test_db();
        save_node(&conn, &sample_node("a", "A")).unwrap();
        save_node(&conn, &sample_node("b", "B")).unwrap();
        save_node(&conn, &sample_node("c", "C")).unwrap();

        let j = Justification::new_sl(
            "c".to_string(),
            vec!["a".to_string(), "b".to_string()],
            vec!["blocker".to_string()],
            "my-label".to_string(),
        );
        let rowid = save_justification(&conn, &j).unwrap();
        assert!(rowid > 0);

        let loaded = load_justifications(&conn, "c").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].jtype, "SL");
        assert_eq!(loaded[0].antecedents, vec!["a", "b"]);
        assert_eq!(loaded[0].outlist, vec!["blocker"]);
        assert_eq!(loaded[0].label, "my-label");
    }

    #[test]
    fn load_all_justifications() {
        let conn = test_db();
        save_node(&conn, &sample_node("a", "A")).unwrap();
        save_node(&conn, &sample_node("b", "B")).unwrap();

        save_justification(&conn, &Justification::new_sl(
            "a".to_string(), vec![], vec![], String::new(),
        )).unwrap();
        save_justification(&conn, &Justification::new_sl(
            "b".to_string(), vec!["a".to_string()], vec![], String::new(),
        )).unwrap();

        let all = super::load_all_justifications(&conn).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn delete_justification_removes_it() {
        let conn = test_db();
        save_node(&conn, &sample_node("a", "A")).unwrap();
        let rowid = save_justification(&conn, &Justification::new_sl(
            "a".to_string(), vec![], vec![], String::new(),
        )).unwrap();

        assert_eq!(load_justifications(&conn, "a").unwrap().len(), 1);
        super::delete_justification(&conn, rowid).unwrap();
        assert_eq!(load_justifications(&conn, "a").unwrap().len(), 0);
    }

    #[test]
    fn update_node_truth_changes_value() {
        let conn = test_db();
        save_node(&conn, &sample_node("a", "A")).unwrap();

        update_node_truth(&conn, "a", "OUT").unwrap();
        let loaded = load_node(&conn, "a").unwrap().unwrap();
        assert_eq!(loaded.truth_value, "OUT");
    }

    #[test]
    fn update_node_metadata_round_trip() {
        let conn = test_db();
        save_node(&conn, &sample_node("a", "A")).unwrap();

        let meta = serde_json::json!({"key": "value", "num": 42});
        update_node_metadata(&conn, "a", &meta).unwrap();

        let loaded = load_node(&conn, "a").unwrap().unwrap();
        assert_eq!(loaded.metadata["key"], "value");
        assert_eq!(loaded.metadata["num"], 42);
    }

    #[test]
    fn save_and_load_nogood_round_trip() {
        let conn = test_db();
        let ng = Nogood {
            id: "nogood-001".to_string(),
            nodes: vec!["a".to_string(), "b".to_string()],
            discovered: "2026-01-01".to_string(),
            resolution: "retracted a".to_string(),
        };
        save_nogood(&conn, &ng).unwrap();

        let loaded = load_nogoods(&conn).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "nogood-001");
        assert_eq!(loaded[0].nodes, vec!["a", "b"]);
        assert_eq!(loaded[0].resolution, "retracted a");
    }

    #[test]
    fn set_and_load_meta() {
        let conn = test_db();
        set_meta(&conn, "project_name", "test-project").unwrap();
        assert_eq!(load_meta(&conn, "project_name").unwrap().unwrap(), "test-project");
        assert!(load_meta(&conn, "missing").unwrap().is_none());
    }

    #[test]
    fn fts_search_finds_nodes() {
        let conn = test_db();
        save_node(&conn, &sample_node("cat-fact", "Cats sleep 16 hours a day")).unwrap();
        save_node(&conn, &sample_node("dog-fact", "Dogs are loyal companions")).unwrap();
        rebuild_fts(&conn).unwrap();

        let results = fts_search(&conn, "\"cats\" \"sleep\"", 10).unwrap();
        assert_eq!(results, vec!["cat-fact"]);
    }

    #[test]
    fn substring_search_matches_text_and_id() {
        let conn = test_db();
        save_node(&conn, &sample_node("alpha-node", "Some text here")).unwrap();
        save_node(&conn, &sample_node("beta-node", "Alpha is mentioned")).unwrap();

        let by_id = substring_search(&conn, "alpha", 10).unwrap();
        assert!(by_id.contains(&"alpha-node".to_string()));

        let by_text = substring_search(&conn, "mentioned", 10).unwrap();
        assert!(by_text.contains(&"beta-node".to_string()));
    }

    #[test]
    fn node_count_tracks_in_out() {
        let conn = test_db();
        save_node(&conn, &sample_node("a", "A")).unwrap();
        let mut b = sample_node("b", "B");
        b.truth_value = "OUT".to_string();
        save_node(&conn, &b).unwrap();

        let (total, in_count) = node_count(&conn).unwrap();
        assert_eq!(total, 2);
        assert_eq!(in_count, 1);
    }

    #[test]
    fn propagation_log_round_trip() {
        let conn = test_db();
        log_propagation(&conn, "retract", "node-1", "OUT").unwrap();
        log_propagation(&conn, "propagate", "node-2", "OUT").unwrap();

        let entries = load_propagation_log(&conn, 10).unwrap();
        assert_eq!(entries.len(), 2);
        // Most recent first
        assert_eq!(entries[0].1, "propagate");
        assert_eq!(entries[1].1, "retract");
    }

    #[test]
    fn load_repos_round_trip() {
        let conn = test_db();
        conn.execute("INSERT INTO repos (name, path) VALUES ('myrepo', '/path/to/repo')", []).unwrap();
        let repos = load_repos(&conn).unwrap();
        assert_eq!(repos.get("myrepo").unwrap(), "/path/to/repo");
    }

    #[test]
    fn delete_node_removes_node_and_justifications() {
        let conn = test_db();
        save_node(&conn, &sample_node("a", "A")).unwrap();
        save_justification(&conn, &Justification::new_sl(
            "a".to_string(), vec![], vec![], String::new(),
        )).unwrap();

        delete_node(&conn, "a").unwrap();
        assert!(load_node(&conn, "a").unwrap().is_none());
        assert!(load_justifications(&conn, "a").unwrap().is_empty());
    }
}

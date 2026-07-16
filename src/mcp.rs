use std::path::Path;
use std::sync::{Arc, Mutex};

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{schemars, tool, tool_router, ServiceExt, transport::stdio};
use rusqlite::Connection;

use crate::{db, format, tms::Network, types::{Justification, Node, Nogood}};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchParams {
    #[schemars(description = "Search query string")]
    query: String,
    #[schemars(description = "Output format: markdown, json, or minimal")]
    #[serde(default = "default_markdown")]
    format: String,
    #[schemars(description = "Neighbor expansion depth (default 1)")]
    #[serde(default = "default_depth")]
    depth: usize,
    #[schemars(description = "Include OUT (retracted) beliefs in results (default: false)")]
    #[serde(default)]
    include_out: bool,
}

fn default_markdown() -> String { "markdown".to_string() }
fn default_depth() -> usize { 1 }

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct NodeIdParams {
    #[schemars(description = "The node ID to look up")]
    node_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct TreeParams {
    #[schemars(description = "The node ID to display the tree for")]
    node_id: String,
    #[schemars(description = "Direction: up (antecedents), down (dependents), or both")]
    #[serde(default = "default_up")]
    direction: String,
    #[schemars(description = "Maximum depth to traverse")]
    max_depth: Option<usize>,
}

fn default_up() -> String { "up".to_string() }

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ListParams {
    #[schemars(description = "Filter by truth value: IN or OUT")]
    status: Option<String>,
    #[schemars(description = "Only show premises (nodes with no justifications)")]
    #[serde(default)]
    premises: bool,
    #[schemars(description = "Only show nodes that have dependents")]
    #[serde(default)]
    has_dependents: bool,
    #[schemars(description = "Sort by number of dependents (most impactful first)")]
    #[serde(default)]
    by_impact: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AddParams {
    #[schemars(description = "Unique identifier for the new belief node")]
    node_id: String,
    #[schemars(description = "The belief text")]
    text: String,
    #[schemars(description = "Comma-separated antecedent node IDs (support list justification)")]
    sl: Option<String>,
    #[schemars(description = "Comma-separated outlist node IDs (unless clause)")]
    unless: Option<String>,
    #[schemars(description = "Source document or reference")]
    source: Option<String>,
    #[schemars(description = "URL of the source")]
    source_url: Option<String>,
    #[schemars(description = "Label for the justification")]
    label: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct RetractParams {
    #[schemars(description = "The node ID to retract")]
    node_id: String,
    #[schemars(description = "Reason for retraction")]
    reason: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ChallengeParams {
    #[schemars(description = "The node ID being challenged")]
    target_id: String,
    #[schemars(description = "Reason for the challenge")]
    reason: String,
    #[schemars(description = "Custom ID for the challenge node (defaults to challenge-<target_id>)")]
    challenge_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct DefendParams {
    #[schemars(description = "The node ID being defended")]
    target_id: String,
    #[schemars(description = "The challenge node ID to counter")]
    challenge_id: String,
    #[schemars(description = "Reason for the defense")]
    reason: String,
    #[schemars(description = "Custom ID for the defense node (defaults to defense-<challenge_id>)")]
    defense_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct NogoodParams {
    #[schemars(description = "List of contradicting node IDs")]
    node_ids: Vec<String>,
}

#[derive(Clone)]
struct ReasonsServer {
    conn: Arc<Mutex<Connection>>,
}

const STOP_WORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "against", "all", "am", "an", "and", "any", "are",
    "as", "at", "be", "because", "been", "before", "being", "below", "between", "both", "but",
    "by", "could", "did", "do", "does", "doing", "down", "during", "each", "few", "for", "from",
    "further", "get", "got", "had", "has", "have", "having", "he", "her", "here", "hers",
    "herself", "him", "himself", "his", "how", "i", "if", "in", "into", "is", "it", "its",
    "itself", "just", "let", "like", "me", "might", "more", "most", "my", "myself", "no", "nor",
    "not", "now", "of", "off", "on", "once", "only", "or", "other", "our", "ours", "ourselves",
    "out", "over", "own", "same", "she", "should", "so", "some", "such", "than", "that", "the",
    "their", "theirs", "them", "themselves", "then", "there", "these", "they", "this", "those",
    "through", "to", "too", "under", "until", "up", "very", "was", "we", "were", "what", "when",
    "where", "which", "while", "who", "whom", "why", "will", "with", "would", "you", "your",
    "yours", "yourself", "yourselves",
];

#[tool_router(server_handler)]
impl ReasonsServer {
    #[tool(description = "Search beliefs using full-text search with neighbor expansion. Returns matching belief nodes with their truth values.")]
    async fn search(&self, Parameters(params): Parameters<SearchParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            search_impl(&conn, &params.query, &params.format, params.depth, params.include_out)
        }).await.unwrap()
    }

    #[tool(description = "Show detailed information about a belief node including its text, source, justifications, and dependents.")]
    async fn show(&self, Parameters(params): Parameters<NodeIdParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            show_impl(&conn, &params.node_id)
        }).await.unwrap()
    }

    #[tool(description = "Explain why a belief node is IN or OUT by tracing through its justification chain.")]
    async fn explain(&self, Parameters(params): Parameters<NodeIdParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            explain_impl(&conn, &params.node_id)
        }).await.unwrap()
    }

    #[tool(description = "Show a dependency tree visualization for a belief node using box-drawing characters.")]
    async fn tree(&self, Parameters(params): Parameters<TreeParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            tree_impl(&conn, &params.node_id, &params.direction, params.max_depth)
        }).await.unwrap()
    }

    #[tool(description = "List belief nodes with optional filters by status, type, and impact.")]
    async fn list(&self, Parameters(params): Parameters<ListParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            list_impl(&conn, params.status.as_deref(), params.premises, params.has_dependents, params.by_impact)
        }).await.unwrap()
    }

    #[tool(description = "Add a new belief node to the truth maintenance system. Can be a premise (no justification) or derived (with --sl antecedents).")]
    async fn add(&self, Parameters(params): Parameters<AddParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            add_impl(&conn, &params.node_id, &params.text, params.sl.as_deref(),
                params.unless.as_deref(), params.source.as_deref(),
                params.source_url.as_deref(), params.label.as_deref())
        }).await.unwrap()
    }

    #[tool(description = "Retract a belief node, marking it OUT with cascading truth-value propagation to dependents.")]
    async fn retract(&self, Parameters(params): Parameters<RetractParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            retract_impl(&conn, &params.node_id, params.reason.as_deref())
        }).await.unwrap()
    }

    #[tool(description = "Re-assert a previously retracted belief node, restoring it to IN with cascading propagation.")]
    async fn assert_node(&self, Parameters(params): Parameters<NodeIdParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            assert_impl(&conn, &params.node_id)
        }).await.unwrap()
    }

    #[tool(description = "Challenge a belief by creating a challenge node that makes the target go OUT.")]
    async fn challenge(&self, Parameters(params): Parameters<ChallengeParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            challenge_impl(&conn, &params.target_id, &params.reason, params.challenge_id.as_deref())
        }).await.unwrap()
    }

    #[tool(description = "Defend a belief against a challenge by creating a defense node that counters the challenge, restoring the original belief to IN.")]
    async fn defend(&self, Parameters(params): Parameters<DefendParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            defend_impl(&conn, &params.target_id, &params.challenge_id, &params.reason, params.defense_id.as_deref())
        }).await.unwrap()
    }

    #[tool(description = "Record a contradiction between belief nodes. If all nodes are IN, runs dependency-directed backtracking to retract the least-entrenched premise.")]
    async fn nogood(&self, Parameters(params): Parameters<NogoodParams>) -> String {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            nogood_impl(&conn, &params.node_ids)
        }).await.unwrap()
    }
}

// --- Implementation functions that return String instead of printing ---

fn search_impl(conn: &Connection, query: &str, output_format: &str, depth: usize, include_out: bool) -> String {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect();

    if words.is_empty() {
        return "No search terms after filtering stop words.".to_string();
    }

    let fts_query = words.iter()
        .map(|w| format!("\"{}\"", w))
        .collect::<Vec<_>>()
        .join(" ");

    let mut matched_ids = db::fts_search(conn, &fts_query, 20).unwrap_or_default();

    if matched_ids.is_empty() && words.len() >= 2 {
        for drop_count in 1..=(words.len() / 2) {
            let subset: Vec<_> = words[..words.len() - drop_count].to_vec();
            let fts_q = subset.iter()
                .map(|w| format!("\"{}\"", w))
                .collect::<Vec<_>>()
                .join(" ");
            matched_ids = db::fts_search(conn, &fts_q, 20).unwrap_or_default();
            if !matched_ids.is_empty() {
                break;
            }
        }
    }

    if matched_ids.is_empty() {
        matched_ids = db::substring_search(conn, query, 20).unwrap_or_default();
    }

    if matched_ids.is_empty() {
        return "No results found.".to_string();
    }

    let expanded = expand_neighbors(conn, &matched_ids, depth);
    let mut nodes = Vec::new();
    for id in &expanded {
        if let Ok(Some(node)) = db::load_node(conn, id) {
            if !include_out && node.truth_value == "OUT" {
                continue;
            }
            nodes.push(node);
        }
    }

    if nodes.is_empty() {
        return "No results found.".to_string();
    }

    match output_format {
        "json" => format::format_nodes_json(&nodes),
        "minimal" => format::format_nodes_minimal(&nodes),
        _ => {
            let mut lines = Vec::new();
            for node in &nodes {
                let justs = db::load_justifications(conn, &node.id).unwrap_or_default();
                let marker = if matched_ids.contains(&node.id) { "*" } else { " " };
                let premise_tag = if justs.is_empty() { " (premise)" } else { "" };
                lines.push(format!("{} [{}] {}: {}{}",
                    marker, node.truth_value, node.id,
                    format::truncate(&node.text, 80), premise_tag));
            }
            lines.join("\n")
        }
    }
}

fn expand_neighbors(conn: &Connection, seed_ids: &[String], depth: usize) -> Vec<String> {
    use std::collections::{HashSet, VecDeque};

    let mut result: Vec<String> = seed_ids.to_vec();
    let mut visited: HashSet<String> = seed_ids.iter().cloned().collect();

    if depth == 0 {
        return result;
    }

    let all_justs = db::load_all_justifications(conn).unwrap_or_default();
    let mut queue: VecDeque<(String, usize)> = seed_ids.iter()
        .map(|id| (id.clone(), 0))
        .collect();

    while let Some((current, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }
        for j in all_justs.iter().filter(|j| j.node_id == current) {
            for ant_id in &j.antecedents {
                if visited.insert(ant_id.clone()) {
                    result.push(ant_id.clone());
                    queue.push_back((ant_id.clone(), d + 1));
                }
            }
        }
        for j in all_justs.iter() {
            if j.antecedents.contains(&current) || j.outlist.contains(&current) {
                if visited.insert(j.node_id.clone()) {
                    result.push(j.node_id.clone());
                    queue.push_back((j.node_id.clone(), d + 1));
                }
            }
        }
    }

    result
}

fn show_impl(conn: &Connection, node_id: &str) -> String {
    let node = match db::load_node(conn, node_id) {
        Ok(Some(n)) => n,
        _ => return format!("Node not found: {}", node_id),
    };

    let mut out = format::format_node_detail(&node);

    let justs = db::load_justifications(conn, node_id).unwrap_or_default();
    if !justs.is_empty() {
        out.push_str("  Justifications:\n");
        for (i, j) in justs.iter().enumerate() {
            let mut parts = format!("    [{}] {}: {}", i, j.jtype, j.antecedents.join(", "));
            if !j.outlist.is_empty() {
                parts.push_str(&std::format!(" (unless: {})", j.outlist.join(", ")));
            }
            if !j.label.is_empty() {
                parts.push_str(&std::format!(" [{}]", j.label));
            }
            out.push_str(&parts);
            out.push('\n');
        }
    }

    let all_justs = db::load_all_justifications(conn).unwrap_or_default();
    let mut dependents: Vec<String> = Vec::new();
    for j in &all_justs {
        if j.antecedents.contains(&node_id.to_string()) || j.outlist.contains(&node_id.to_string()) {
            if !dependents.contains(&j.node_id) {
                dependents.push(j.node_id.clone());
            }
        }
    }

    if !dependents.is_empty() {
        out.push_str("\n  Dependents:\n");
        for dep_id in &dependents {
            if let Ok(Some(dep)) = db::load_node(conn, dep_id) {
                out.push_str(&format!("    {} [{}]: {}\n",
                    dep_id, dep.truth_value, format::truncate(&dep.text, 60)));
            }
        }
    }

    out
}

fn explain_impl(conn: &Connection, node_id: &str) -> String {
    let mut out = String::new();
    let mut visited = std::collections::HashSet::new();
    explain_recursive(conn, node_id, &mut visited, 0, &mut out);
    out
}

fn explain_recursive(
    conn: &Connection,
    node_id: &str,
    visited: &mut std::collections::HashSet<String>,
    depth: usize,
    out: &mut String,
) {
    let indent = "  ".repeat(depth);

    if !visited.insert(node_id.to_string()) {
        out.push_str(&format!("{}{} (circular)\n", indent, node_id));
        return;
    }

    let node = match db::load_node(conn, node_id) {
        Ok(Some(n)) => n,
        _ => {
            out.push_str(&format!("{}{} (not found)\n", indent, node_id));
            return;
        }
    };

    let justs = db::load_justifications(conn, node_id).unwrap_or_default();
    if justs.is_empty() {
        out.push_str(&format!("{}{} is {} (premise)\n", indent, node_id, node.truth_value));
        return;
    }

    if node.truth_value == "IN" {
        if let Some(valid_j) = justs.iter().find(|j| {
            let inlist_ok = j.antecedents.iter().all(|a| {
                db::load_node(conn, a).ok().flatten().map_or(false, |n| n.truth_value == "IN")
            });
            let outlist_ok = j.outlist.iter().all(|o| {
                db::load_node(conn, o).ok().flatten().map_or(true, |n| n.truth_value == "OUT")
            });
            inlist_ok && outlist_ok
        }) {
            let mut desc = format!("Justified by {}: {}", valid_j.jtype, valid_j.antecedents.join(", "));
            if !valid_j.outlist.is_empty() {
                desc.push_str(&std::format!(" (unless: {})", valid_j.outlist.join(", ")));
            }
            out.push_str(&format!("{}{} is IN because:\n", indent, node_id));
            out.push_str(&format!("{}  {}\n", indent, desc));
            for ant_id in &valid_j.antecedents {
                explain_recursive(conn, ant_id, visited, depth + 2, out);
            }
        }
    } else {
        out.push_str(&format!("{}{} is OUT because:\n", indent, node_id));
        for j in &justs {
            let mut reasons = Vec::new();
            for ant_id in &j.antecedents {
                if let Ok(Some(ant)) = db::load_node(conn, ant_id) {
                    if ant.truth_value == "OUT" {
                        reasons.push(format!("{} is OUT", ant_id));
                    }
                } else {
                    reasons.push(format!("{} not found", ant_id));
                }
            }
            for out_id in &j.outlist {
                if let Ok(Some(out_node)) = db::load_node(conn, out_id) {
                    if out_node.truth_value == "IN" {
                        reasons.push(format!("{} is IN (in outlist)", out_id));
                    }
                }
            }
            if reasons.is_empty() {
                reasons.push("unknown reason".to_string());
            }
            out.push_str(&format!("{}  {}: {} — {}\n",
                indent, j.jtype, j.antecedents.join(", "), reasons.join("; ")));
        }
    }
}

fn tree_impl(conn: &Connection, node_id: &str, direction: &str, max_depth: Option<usize>) -> String {
    let nodes = db::load_all_nodes(conn).unwrap_or_default();
    let justs = db::load_all_justifications(conn).unwrap_or_default();
    let net = Network::load(nodes, justs);

    let node = match net.nodes.get(node_id) {
        Some(n) => n,
        None => return format!("Node not found: {}", node_id),
    };

    let mut out = String::new();
    let is_premise = net.justifications.get(node_id).map_or(true, |v| v.is_empty());
    let premise_tag = if is_premise { " (premise)" } else { "" };
    out.push_str(&format!("{} [{}]: {}{}\n",
        node.id, node.truth_value, format::truncate(&node.text, 70), premise_tag));

    let mut visited = std::collections::HashSet::new();
    visited.insert(node_id.to_string());

    match direction {
        "up" | "both" => {
            let children = get_antecedents(&net, node_id);
            tree_subtree(&net, &children, "up", 1, max_depth, &mut visited, "", &mut out);
        }
        _ => {}
    }

    match direction {
        "down" | "both" => {
            let children = get_dependents(&net, node_id);
            tree_subtree(&net, &children, "down", 1, max_depth, &mut visited, "", &mut out);
        }
        _ => {}
    }

    out
}

fn get_antecedents(net: &Network, node_id: &str) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(justs) = net.justifications.get(node_id) {
        for j in justs {
            for ant_id in &j.antecedents {
                if !result.contains(ant_id) {
                    result.push(ant_id.clone());
                }
            }
        }
    }
    result
}

fn get_dependents(net: &Network, node_id: &str) -> Vec<String> {
    net.dependents.get(node_id)
        .map(|d| d.iter().cloned().collect())
        .unwrap_or_default()
}

fn tree_subtree(
    net: &Network,
    children: &[String],
    direction: &str,
    depth: usize,
    max_depth: Option<usize>,
    visited: &mut std::collections::HashSet<String>,
    prefix: &str,
    out: &mut String,
) {
    if let Some(max) = max_depth {
        if depth > max {
            return;
        }
    }

    for (i, child_id) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };

        if !visited.insert(child_id.clone()) {
            out.push_str(&format!("{}{}{} (circular)\n", prefix, connector, child_id));
            continue;
        }

        if let Some(node) = net.nodes.get(child_id) {
            let is_premise = net.justifications.get(child_id).map_or(true, |v| v.is_empty());
            let premise_tag = if is_premise { " (premise)" } else { "" };
            out.push_str(&format!("{}{}{} [{}]: {}{}\n",
                prefix, connector, node.id, node.truth_value,
                format::truncate(&node.text, 60), premise_tag));

            let grandchildren = match direction {
                "up" => get_antecedents(net, child_id),
                "down" => get_dependents(net, child_id),
                _ => Vec::new(),
            };

            if !grandchildren.is_empty() {
                let child_prefix = if is_last {
                    format!("{}    ", prefix)
                } else {
                    format!("{}│   ", prefix)
                };
                tree_subtree(net, &grandchildren, direction, depth + 1, max_depth,
                    visited, &child_prefix, out);
            }
        }

        visited.remove(child_id);
    }
}

fn list_impl(
    conn: &Connection,
    status: Option<&str>,
    premises: bool,
    has_dependents: bool,
    by_impact: bool,
) -> String {
    let all_nodes = db::load_all_nodes(conn).unwrap_or_default();
    let all_justs = db::load_all_justifications(conn).unwrap_or_default();

    let nodes_with_justs: std::collections::HashSet<String> =
        all_justs.iter().map(|j| j.node_id.clone()).collect();

    let mut dep_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for j in &all_justs {
        for ant_id in &j.antecedents {
            *dep_counts.entry(ant_id.clone()).or_default() += 1;
        }
        for out_id in &j.outlist {
            *dep_counts.entry(out_id.clone()).or_default() += 1;
        }
    }

    let mut filtered: Vec<_> = all_nodes.iter().filter(|n| {
        if let Some(s) = status {
            if n.truth_value != s {
                return false;
            }
        }
        if premises && nodes_with_justs.contains(&n.id) {
            return false;
        }
        if has_dependents && dep_counts.get(&n.id).unwrap_or(&0) == &0 {
            return false;
        }
        true
    }).collect();

    if by_impact {
        filtered.sort_by(|a, b| {
            let da = dep_counts.get(&a.id).unwrap_or(&0);
            let db_count = dep_counts.get(&b.id).unwrap_or(&0);
            db_count.cmp(da)
        });
    }

    filtered.iter()
        .map(|n| format::format_node_line(n))
        .collect::<Vec<_>>()
        .join("\n")
}

fn add_impl(
    conn: &Connection,
    node_id: &str,
    text: &str,
    sl: Option<&str>,
    unless: Option<&str>,
    source: Option<&str>,
    source_url: Option<&str>,
    label: Option<&str>,
) -> String {
    if let Ok(Some(_)) = db::load_node(conn, node_id) {
        return format!("Error: Node already exists: {}", node_id);
    }

    let mut node = Node::new(node_id.to_string(), text.to_string());
    if let Some(s) = source {
        node.source = s.to_string();
    }
    if let Some(u) = source_url {
        node.source_url = u.to_string();
    }

    if let Some(deps) = sl {
        let antecedents: Vec<String> = deps.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let outlist: Vec<String> = unless
            .map(|u| u.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();

        let all_nodes = db::load_all_nodes(conn).unwrap_or_default();
        let all_justs = db::load_all_justifications(conn).unwrap_or_default();
        let mut net = Network::load(all_nodes, all_justs);

        net.nodes.insert(node_id.to_string(), node.clone());
        let j = Justification::new_sl(
            node_id.to_string(),
            antecedents,
            outlist,
            label.unwrap_or("").to_string(),
        );

        let changed = net.add_justification(node_id, j.clone());

        if let Some(n) = net.nodes.get(node_id) {
            node.truth_value = n.truth_value.clone();
        }
        if db::save_node(conn, &node).is_err() {
            return format!("Error: Failed to save node {}", node_id);
        }
        let _ = db::save_justification(conn, &j);

        for cid in &changed {
            if cid != node_id {
                if let Some(n) = net.nodes.get(cid) {
                    let _ = db::update_node_truth(conn, cid, &n.truth_value);
                    let _ = db::log_propagation(conn, "propagate", cid, &n.truth_value);
                }
            }
        }
        let _ = db::log_propagation(conn, "add", node_id, &node.truth_value);
    } else {
        if db::save_node(conn, &node).is_err() {
            return format!("Error: Failed to save node {}", node_id);
        }
        let _ = db::log_propagation(conn, "add", node_id, "IN");
    }

    let _ = db::rebuild_fts(conn);
    let now = chrono::Utc::now().to_rfc3339();
    let _ = db::set_meta(conn, "updated_at", &now);

    format!("Added {} [{}]", node_id, node.truth_value)
}

fn retract_impl(conn: &Connection, node_id: &str, reason: Option<&str>) -> String {
    if db::load_node(conn, node_id).ok().flatten().is_none() {
        return format!("Error: Node not found: {}", node_id);
    }

    let all_nodes = db::load_all_nodes(conn).unwrap_or_default();
    let all_justs = db::load_all_justifications(conn).unwrap_or_default();
    let mut net = Network::load(all_nodes, all_justs);

    let cascaded = net.retract(node_id, reason);

    if let Some(node) = net.nodes.get(node_id) {
        let _ = db::save_node(conn, node);
        let _ = db::log_propagation(conn, "retract", node_id, "OUT");
    }

    for cid in &cascaded {
        if let Some(n) = net.nodes.get(cid) {
            let _ = db::update_node_truth(conn, cid, &n.truth_value);
            let _ = db::log_propagation(conn, "propagate", cid, &n.truth_value);
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let _ = db::set_meta(conn, "updated_at", &now);

    let mut out = format!("Retracted {}", node_id);
    if !cascaded.is_empty() {
        let summary: Vec<String> = cascaded.iter().map(|id| {
            let tv = net.nodes.get(id).map_or("?", |n| &n.truth_value);
            format!("{} {}", id, tv)
        }).collect();
        out.push_str(&format!("\n  Cascaded: {}", summary.join(", ")));
    }
    out
}

fn assert_impl(conn: &Connection, node_id: &str) -> String {
    if db::load_node(conn, node_id).ok().flatten().is_none() {
        return format!("Error: Node not found: {}", node_id);
    }

    let all_nodes = db::load_all_nodes(conn).unwrap_or_default();
    let all_justs = db::load_all_justifications(conn).unwrap_or_default();
    let mut net = Network::load(all_nodes, all_justs);

    let cascaded = net.assert_node(node_id);

    if let Some(node) = net.nodes.get(node_id) {
        let _ = db::save_node(conn, node);
        let _ = db::log_propagation(conn, "assert", node_id, "IN");
    }

    for cid in &cascaded {
        if let Some(n) = net.nodes.get(cid) {
            let _ = db::update_node_truth(conn, cid, &n.truth_value);
            let _ = db::log_propagation(conn, "propagate", cid, &n.truth_value);
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let _ = db::set_meta(conn, "updated_at", &now);

    let mut out = format!("Asserted {}", node_id);
    if !cascaded.is_empty() {
        let summary: Vec<String> = cascaded.iter().map(|id| {
            let tv = net.nodes.get(id).map_or("?", |n| &n.truth_value);
            format!("{} {}", id, tv)
        }).collect();
        out.push_str(&format!("\n  Cascaded: {}", summary.join(", ")));
    }
    out
}

fn challenge_impl(
    conn: &Connection,
    target_id: &str,
    reason: &str,
    challenge_id: Option<&str>,
) -> String {
    if db::load_node(conn, target_id).ok().flatten().is_none() {
        return format!("Error: Node not found: {}", target_id);
    }

    let cid = challenge_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("challenge-{}", target_id));

    let challenge_node = Node::new(cid.clone(), reason.to_string());
    if db::save_node(conn, &challenge_node).is_err() {
        return format!("Error: Failed to create challenge node {}", cid);
    }

    let all_nodes = db::load_all_nodes(conn).unwrap_or_default();
    let all_justs = db::load_all_justifications(conn).unwrap_or_default();
    let mut net = Network::load(all_nodes, all_justs);

    let target_justs = net.justifications.get(target_id).cloned().unwrap_or_default();
    let mut all_changed = Vec::new();

    if target_justs.is_empty() {
        let j = Justification::new_sl(
            target_id.to_string(),
            Vec::new(),
            vec![cid.clone()],
            String::new(),
        );
        let _ = db::save_justification(conn, &j);
        all_changed.extend(net.add_justification(target_id, j));
    } else {
        let _ = conn.execute(
            "DELETE FROM justifications WHERE node_id = ?1",
            rusqlite::params![target_id],
        );

        for mut j in target_justs {
            if !j.outlist.contains(&cid) {
                j.outlist.push(cid.clone());
            }
            let _ = db::save_justification(conn, &j);
        }

        let reloaded_justs = db::load_justifications(conn, target_id).unwrap_or_default();
        net.justifications.insert(target_id.to_string(), reloaded_justs);
        net.rebuild_dependents();

        let new_truth = net.compute_truth(target_id).to_string();
        if let Some(target) = net.nodes.get_mut(target_id) {
            if target.truth_value != new_truth {
                target.truth_value = new_truth;
                all_changed.push(target_id.to_string());
                all_changed.extend(net.propagate(target_id));
            }
        }
    }

    if let Some(target) = net.nodes.get_mut(target_id) {
        if let Some(obj) = target.metadata.as_object_mut() {
            let challenges = obj.entry("challenges".to_string())
                .or_insert_with(|| serde_json::json!([]));
            if let Some(arr) = challenges.as_array_mut() {
                arr.push(serde_json::json!(cid));
            }
        }
        let _ = db::update_node_metadata(conn, target_id, &target.metadata);
    }

    for changed_id in &all_changed {
        if let Some(n) = net.nodes.get(changed_id) {
            let _ = db::update_node_truth(conn, changed_id, &n.truth_value);
            let _ = db::log_propagation(conn, "propagate", changed_id, &n.truth_value);
        }
    }

    let _ = db::log_propagation(conn, "challenge", target_id,
        &net.nodes.get(target_id).map_or("OUT", |n| &n.truth_value).to_string());
    let _ = db::rebuild_fts(conn);
    let now = chrono::Utc::now().to_rfc3339();
    let _ = db::set_meta(conn, "updated_at", &now);

    format!("Challenged {} with {} -> target is now {}",
        target_id, cid,
        net.nodes.get(target_id).map_or("?", |n| &n.truth_value))
}

fn defend_impl(
    conn: &Connection,
    target_id: &str,
    challenge_id: &str,
    reason: &str,
    defense_id: Option<&str>,
) -> String {
    let did = defense_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("defense-{}", challenge_id));

    let challenge_result = challenge_impl(conn, challenge_id, reason, Some(&did));

    if let Ok(Some(mut defense)) = db::load_node(conn, &did) {
        if let Some(obj) = defense.metadata.as_object_mut() {
            obj.insert("defense_target".to_string(), serde_json::json!(target_id));
            obj.insert("defends".to_string(), serde_json::json!(challenge_id));
        }
        let _ = db::update_node_metadata(conn, &did, &defense.metadata);
    }

    format!("{}\nDefended {} against {} with {}", challenge_result, target_id, challenge_id, did)
}

fn nogood_impl(conn: &Connection, node_ids: &[String]) -> String {
    for id in node_ids {
        if db::load_node(conn, id).ok().flatten().is_none() {
            return format!("Error: Node not found: {}", id);
        }
    }

    let existing = db::load_nogoods(conn).unwrap_or_default();
    let next_id = existing.len() + 1;
    let nogood_id = format!("nogood-{:03}", next_id);

    let now = chrono::Utc::now().to_rfc3339();
    let mut nogood = Nogood {
        id: nogood_id.clone(),
        nodes: node_ids.to_vec(),
        discovered: now.clone(),
        resolution: String::new(),
    };
    if db::save_nogood(conn, &nogood).is_err() {
        return format!("Error: Failed to save nogood {}", nogood_id);
    }

    let mut out = format!("Recorded contradiction: {}\n  Nodes: {}", nogood_id, node_ids.join(", "));

    let all_nodes = db::load_all_nodes(conn).unwrap_or_default();
    let all_justs = db::load_all_justifications(conn).unwrap_or_default();
    let mut net = Network::load(all_nodes, all_justs);

    let all_in = node_ids.iter().all(|id| {
        net.nodes.get(id).map_or(false, |n| n.truth_value == "IN")
    });

    if all_in {
        out.push_str("\n  All nodes are IN — contradiction is active. Running backtracking...");
        let culprits = net.find_culprits(node_ids);

        if let Some((least_entrenched, score)) = culprits.first() {
            let cascaded = net.retract(least_entrenched, Some("Retracted by dependency-directed backtracking"));

            if let Some(node) = net.nodes.get(least_entrenched) {
                let _ = db::save_node(conn, node);
                let _ = db::log_propagation(conn, "backtrack-retract", least_entrenched, "OUT");
            }

            for cid in &cascaded {
                if let Some(n) = net.nodes.get(cid) {
                    let _ = db::update_node_truth(conn, cid, &n.truth_value);
                    let _ = db::log_propagation(conn, "propagate", cid, &n.truth_value);
                }
            }

            let resolution = format!("Retracted {} (entrenchment: {})", least_entrenched, score);
            nogood.resolution = resolution;
            let _ = db::save_nogood(conn, &nogood);

            out.push_str(&format!("\n  Retracted {} (entrenchment: {})", least_entrenched, score));
            if !cascaded.is_empty() {
                out.push_str(&format!("\n  Cascaded: {}", cascaded.join(", ")));
            }
        } else {
            out.push_str("\n  No culprit found for backtracking.");
        }
    } else {
        out.push_str("\n  Not all nodes are IN — contradiction is not currently active.");
    }

    let _ = db::set_meta(conn, "updated_at", &now);
    out
}

pub async fn run_server(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let conn = db::open_db(db_path)?;
    let server = ReasonsServer {
        conn: Arc::new(Mutex::new(conn)),
    };
    let service = server.serve(stdio()).await
        .map_err(|e| format!("Failed to start MCP server: {}", e))?;
    service.waiting().await
        .map_err(|e| format!("MCP server error: {}", e))?;
    Ok(())
}

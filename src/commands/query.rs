use rusqlite::Connection;
use std::collections::{HashSet, VecDeque};
use crate::db;
use crate::format;

pub fn cmd_show(conn: &Connection, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let node = match db::load_node(conn, node_id)? {
        Some(n) => n,
        None => {
            eprintln!("Node not found: {}", node_id);
            std::process::exit(1);
        }
    };

    print!("{}", format::format_node_detail(&node));

    let justs = db::load_justifications(conn, node_id)?;
    if !justs.is_empty() {
        println!("  Justifications:");
        for (i, j) in justs.iter().enumerate() {
            let mut parts = format!("    [{}] {}: {}", i, j.jtype, j.antecedents.join(", "));
            if !j.outlist.is_empty() {
                parts.push_str(&std::format!(" (unless: {})", j.outlist.join(", ")));
            }
            if !j.label.is_empty() {
                parts.push_str(&std::format!(" [{}]", j.label));
            }
            println!("{}", parts);
        }
    }

    let all_justs = db::load_all_justifications(conn)?;
    let mut dependents: Vec<String> = Vec::new();
    for j in &all_justs {
        if j.antecedents.contains(&node_id.to_string()) || j.outlist.contains(&node_id.to_string()) {
            if !dependents.contains(&j.node_id) {
                dependents.push(j.node_id.clone());
            }
        }
    }

    if !dependents.is_empty() {
        println!("\n  Dependents:");
        for dep_id in &dependents {
            if let Some(dep) = db::load_node(conn, dep_id)? {
                println!("    {} [{}]: {}", dep_id, dep.truth_value, format::truncate(&dep.text, 60));
            }
        }
    }

    Ok(())
}

pub fn cmd_explain(conn: &Connection, node_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let node = match db::load_node(conn, node_id)? {
        Some(n) => n,
        None => {
            eprintln!("Node not found: {}", node_id);
            std::process::exit(1);
        }
    };

    let mut visited = HashSet::new();
    explain_recursive(conn, &node.id, &mut visited, 0)?;
    Ok(())
}

fn explain_recursive(
    conn: &Connection,
    node_id: &str,
    visited: &mut HashSet<String>,
    depth: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let indent = "  ".repeat(depth);

    if !visited.insert(node_id.to_string()) {
        println!("{}{} (circular)", indent, node_id);
        return Ok(());
    }

    let node = match db::load_node(conn, node_id)? {
        Some(n) => n,
        None => {
            println!("{}{} (not found)", indent, node_id);
            return Ok(());
        }
    };

    let justs = db::load_justifications(conn, node_id)?;
    if justs.is_empty() {
        println!("{}{} is {} (premise)", indent, node_id, node.truth_value);
        return Ok(());
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
            println!("{}{} is IN because:", indent, node_id);
            println!("{}  {}", indent, desc);
            for ant_id in &valid_j.antecedents {
                explain_recursive(conn, ant_id, visited, depth + 2)?;
            }
        }
    } else {
        println!("{}{} is OUT because:", indent, node_id);
        for j in &justs {
            let mut reasons = Vec::new();
            for ant_id in &j.antecedents {
                if let Some(ant) = db::load_node(conn, ant_id)? {
                    if ant.truth_value == "OUT" {
                        reasons.push(format!("{} is OUT", ant_id));
                    }
                } else {
                    reasons.push(format!("{} not found", ant_id));
                }
            }
            for out_id in &j.outlist {
                if let Some(out_node) = db::load_node(conn, out_id)? {
                    if out_node.truth_value == "IN" {
                        reasons.push(format!("{} is IN (in outlist)", out_id));
                    }
                }
            }
            if reasons.is_empty() {
                reasons.push("unknown reason".to_string());
            }
            println!("{}  {}: {} — {}", indent, j.jtype, j.antecedents.join(", "), reasons.join("; "));
        }
    }

    Ok(())
}

const STOP_WORDS: &[&str] = &[
    "a", "about", "above", "after", "again", "against", "all", "am", "an", "and", "any", "are",
    "aren't", "as", "at", "be", "because", "been", "before", "being", "below", "between", "both",
    "but", "by", "can't", "cannot", "could", "couldn't", "did", "didn't", "do", "does", "doesn't",
    "doing", "don't", "down", "during", "each", "few", "for", "from", "further", "get", "got",
    "had", "hadn't", "has", "hasn't", "have", "haven't", "having", "he", "her", "here", "hers",
    "herself", "him", "himself", "his", "how", "i", "if", "in", "into", "is", "isn't", "it",
    "its", "itself", "just", "let", "like", "ll", "me", "might", "more", "most", "mustn't", "my",
    "myself", "no", "nor", "not", "now", "of", "off", "on", "once", "only", "or", "other", "our",
    "ours", "ourselves", "out", "over", "own", "re", "s", "same", "shan't", "she", "should",
    "shouldn't", "so", "some", "such", "t", "than", "that", "the", "their", "theirs", "them",
    "themselves", "then", "there", "these", "they", "this", "those", "through", "to", "too",
    "under", "until", "up", "ve", "very", "was", "wasn't", "we", "were", "weren't", "what",
    "when", "where", "which", "while", "who", "whom", "why", "will", "with", "won't", "would",
    "wouldn't", "you", "your", "yours", "yourself", "yourselves",
];

pub fn cmd_search(
    conn: &Connection,
    query: &str,
    output_format: &str,
    depth: usize,
    include_out: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect();

    if words.is_empty() {
        println!("No search terms after filtering stop words.");
        return Ok(());
    }

    let fts_query = words.iter()
        .map(|w| format!("\"{}\"", w))
        .collect::<Vec<_>>()
        .join(" ");

    let mut matched_ids = db::fts_search(conn, &fts_query, 20)?;

    if matched_ids.is_empty() && words.len() >= 2 {
        for drop_count in 1..=(words.len() / 2) {
            let subset: Vec<_> = words[..words.len() - drop_count].to_vec();
            let fts_q = subset.iter()
                .map(|w| format!("\"{}\"", w))
                .collect::<Vec<_>>()
                .join(" ");
            matched_ids = db::fts_search(conn, &fts_q, 20)?;
            if !matched_ids.is_empty() {
                break;
            }
        }
    }

    if matched_ids.is_empty() {
        matched_ids = db::substring_search(conn, query, 20)?;
    }

    if matched_ids.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    let expanded = expand_neighbors(conn, &matched_ids, depth)?;

    let mut nodes = Vec::new();
    for id in &expanded {
        if let Some(node) = db::load_node(conn, id)? {
            if !include_out && node.truth_value == "OUT" {
                continue;
            }
            nodes.push(node);
        }
    }

    if nodes.is_empty() {
        println!("No results found.");
        return Ok(());
    }

    match output_format {
        "json" => println!("{}", format::format_nodes_json(&nodes)),
        "minimal" => println!("{}", format::format_nodes_minimal(&nodes)),
        _ => {
            for node in &nodes {
                let justs = db::load_justifications(conn, &node.id)?;
                let marker = if matched_ids.contains(&node.id) { "*" } else { " " };
                let premise_tag = if justs.is_empty() { " (premise)" } else { "" };
                println!("{} [{}] {}: {}{}", marker, node.truth_value, node.id,
                    format::truncate(&node.text, 80), premise_tag);
            }
        }
    }

    Ok(())
}

fn expand_neighbors(
    conn: &Connection,
    seed_ids: &[String],
    depth: usize,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut result: Vec<String> = seed_ids.to_vec();
    let mut visited: HashSet<String> = seed_ids.iter().cloned().collect();

    if depth == 0 {
        return Ok(result);
    }

    let all_justs = db::load_all_justifications(conn)?;

    let mut queue: VecDeque<(String, usize)> = seed_ids.iter()
        .map(|id| (id.clone(), 0))
        .collect();

    while let Some((current, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }
        let justs = all_justs.iter().filter(|j| j.node_id == current);
        for j in justs {
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

    Ok(result)
}

pub fn cmd_lookup(conn: &Connection, query: &str, include_out: bool) -> Result<(), Box<dyn std::error::Error>> {
    let ids = db::substring_search(conn, query, 20)?;
    if ids.is_empty() {
        println!("No results found.");
        return Ok(());
    }
    for id in &ids {
        if let Some(node) = db::load_node(conn, id)? {
            if !include_out && node.truth_value == "OUT" {
                continue;
            }
            println!("[{}] {}: {}", node.truth_value, node.id, format::truncate(&node.text, 80));
        }
    }
    Ok(())
}

pub fn cmd_list(
    conn: &Connection,
    status: Option<&str>,
    premises: bool,
    has_dependents: bool,
    by_impact: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let all_nodes = db::load_all_nodes(conn)?;
    let all_justs = db::load_all_justifications(conn)?;

    let nodes_with_justs: HashSet<String> = all_justs.iter().map(|j| j.node_id.clone()).collect();

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
            let db = dep_counts.get(&b.id).unwrap_or(&0);
            db.cmp(da)
        });
    }

    for node in &filtered {
        println!("{}", format::format_node_line(node));
    }

    Ok(())
}

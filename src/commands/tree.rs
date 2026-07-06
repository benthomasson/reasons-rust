use rusqlite::Connection;
use std::collections::HashSet;
use crate::db;
use crate::format;
use crate::tms::Network;

pub fn cmd_tree(
    conn: &Connection,
    node_id: &str,
    direction: &str,
    max_depth: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let nodes = db::load_all_nodes(conn)?;
    let justs = db::load_all_justifications(conn)?;
    let net = Network::load(nodes, justs);

    let node = match net.nodes.get(node_id) {
        Some(n) => n,
        None => {
            eprintln!("Node not found: {}", node_id);
            std::process::exit(1);
        }
    };

    let is_premise = net.justifications.get(node_id).map_or(true, |v| v.is_empty());
    let premise_tag = if is_premise { " (premise)" } else { "" };
    println!("{} [{}]: {}{}", node.id, node.truth_value, format::truncate(&node.text, 70), premise_tag);

    let mut visited = HashSet::new();
    visited.insert(node_id.to_string());

    match direction {
        "up" | "both" => {
            let children = get_antecedents(&net, node_id);
            print_subtree(&net, &children, "up", 1, max_depth, &mut visited, direction == "both");
        }
        _ => {}
    }

    match direction {
        "down" | "both" => {
            let children = get_dependents(&net, node_id);
            print_subtree(&net, &children, "down", 1, max_depth, &mut visited, false);
        }
        _ => {}
    }

    Ok(())
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

fn print_subtree(
    net: &Network,
    children: &[String],
    direction: &str,
    depth: usize,
    max_depth: Option<usize>,
    visited: &mut HashSet<String>,
    _has_more_sections: bool,
) {
    if let Some(max) = max_depth {
        if depth > max {
            return;
        }
    }

    for (i, child_id) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let prefix = build_prefix(depth - 1, is_last);

        if !visited.insert(child_id.clone()) {
            println!("{}{}{} (circular)", prefix, connector, child_id);
            continue;
        }

        if let Some(node) = net.nodes.get(child_id) {
            let is_premise = net.justifications.get(child_id).map_or(true, |v| v.is_empty());
            let premise_tag = if is_premise { " (premise)" } else { "" };
            println!("{}{}{} [{}]: {}{}",
                prefix, connector, node.id, node.truth_value,
                format::truncate(&node.text, 60), premise_tag);

            let grandchildren = match direction {
                "up" => get_antecedents(net, child_id),
                "down" => get_dependents(net, child_id),
                _ => Vec::new(),
            };

            if !grandchildren.is_empty() {
                let child_prefix = if is_last { "    " } else { "│   " };
                let mut sub_children = Vec::new();
                for gc in &grandchildren {
                    sub_children.push(gc.clone());
                }
                print_subtree_with_prefix(
                    net, &sub_children, direction, depth + 1, max_depth,
                    visited, &format!("{}{}", prefix, child_prefix),
                );
            }
        }

        visited.remove(child_id);
    }
}

fn print_subtree_with_prefix(
    net: &Network,
    children: &[String],
    direction: &str,
    depth: usize,
    max_depth: Option<usize>,
    visited: &mut HashSet<String>,
    prefix: &str,
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
            println!("{}{}{} (circular)", prefix, connector, child_id);
            continue;
        }

        if let Some(node) = net.nodes.get(child_id) {
            let is_premise = net.justifications.get(child_id).map_or(true, |v| v.is_empty());
            let premise_tag = if is_premise { " (premise)" } else { "" };
            println!("{}{}{} [{}]: {}{}",
                prefix, connector, node.id, node.truth_value,
                format::truncate(&node.text, 60), premise_tag);

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
                print_subtree_with_prefix(
                    net, &grandchildren, direction, depth + 1, max_depth,
                    visited, &child_prefix,
                );
            }
        }

        visited.remove(child_id);
    }
}

fn build_prefix(depth: usize, _is_last: bool) -> String {
    "│   ".repeat(depth)
}

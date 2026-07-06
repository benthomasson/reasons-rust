use crate::types::Node;

pub fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

pub fn format_node_line(node: &Node) -> String {
    format!("[{}] {}: {}", node.truth_value, node.id, truncate(&node.text, 80))
}

pub fn format_node_detail(node: &Node) -> String {
    let mut out = format!("{} [{}]\n", node.id, node.truth_value);
    out.push_str(&format!("  Text: {}\n", node.text));
    if !node.source.is_empty() {
        out.push_str(&format!("  Source: {}\n", node.source));
    }
    if !node.source_url.is_empty() {
        out.push_str(&format!("  Source URL: {}\n", node.source_url));
    }
    if !node.created_at.is_empty() {
        out.push_str(&format!("  Created: {}\n", node.created_at));
    }
    if !node.updated_at.is_empty() {
        out.push_str(&format!("  Updated: {}\n", node.updated_at));
    }
    out
}

pub fn format_nodes_json(nodes: &[Node]) -> String {
    serde_json::to_string_pretty(nodes).unwrap_or_else(|_| "[]".to_string())
}

pub fn format_nodes_minimal(nodes: &[Node]) -> String {
    nodes.iter()
        .map(|n| format!("{} [{}]", n.id, n.truth_value))
        .collect::<Vec<_>>()
        .join("\n")
}

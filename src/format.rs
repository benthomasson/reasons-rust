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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Node;

    fn node(id: &str, text: &str) -> Node {
        Node::new(id.to_string(), text.to_string())
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("12345", 5), "12345");
    }

    #[test]
    fn truncate_long_string_gets_ellipsis() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 5), "");
    }

    #[test]
    fn format_node_line_includes_fields() {
        let n = node("test-id", "Some belief text");
        let line = format_node_line(&n);
        assert!(line.contains("[IN]"));
        assert!(line.contains("test-id"));
        assert!(line.contains("Some belief text"));
    }

    #[test]
    fn format_node_detail_includes_text_and_timestamps() {
        let n = node("test-id", "Belief text");
        let detail = format_node_detail(&n);
        assert!(detail.contains("test-id [IN]"));
        assert!(detail.contains("Text: Belief text"));
        assert!(detail.contains("Created:"));
    }

    #[test]
    fn format_node_detail_includes_source_when_set() {
        let mut n = node("x", "text");
        n.source = "doc.md".to_string();
        let detail = format_node_detail(&n);
        assert!(detail.contains("Source: doc.md"));
    }

    #[test]
    fn format_node_detail_omits_empty_source() {
        let n = node("x", "text");
        let detail = format_node_detail(&n);
        assert!(!detail.contains("Source:"));
    }

    #[test]
    fn format_nodes_minimal_one_line_per_node() {
        let nodes = vec![node("a", "A"), node("b", "B")];
        let output = format_nodes_minimal(&nodes);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("a [IN]"));
        assert!(lines[1].contains("b [IN]"));
    }
}

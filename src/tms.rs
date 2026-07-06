use std::collections::{HashMap, HashSet, VecDeque};
use crate::types::{Node, Justification};

pub struct Network {
    pub nodes: HashMap<String, Node>,
    pub justifications: HashMap<String, Vec<Justification>>,
    pub dependents: HashMap<String, HashSet<String>>,
}

impl Network {
    pub fn new() -> Self {
        Network {
            nodes: HashMap::new(),
            justifications: HashMap::new(),
            dependents: HashMap::new(),
        }
    }

    pub fn load(nodes: Vec<Node>, justifications: Vec<Justification>) -> Self {
        let mut net = Network::new();
        for node in nodes {
            net.nodes.insert(node.id.clone(), node);
        }
        for j in justifications {
            net.justifications
                .entry(j.node_id.clone())
                .or_default()
                .push(j);
        }
        net.rebuild_dependents();
        net
    }

    pub fn rebuild_dependents(&mut self) {
        self.dependents.clear();
        for justs in self.justifications.values() {
            for j in justs {
                for ant_id in &j.antecedents {
                    self.dependents
                        .entry(ant_id.clone())
                        .or_default()
                        .insert(j.node_id.clone());
                }
                for out_id in &j.outlist {
                    self.dependents
                        .entry(out_id.clone())
                        .or_default()
                        .insert(j.node_id.clone());
                }
            }
        }
    }

    pub fn justification_valid(&self, j: &Justification) -> bool {
        let inlist_ok = j.antecedents.iter().all(|a| {
            self.nodes.get(a).map_or(false, |n| n.truth_value == "IN")
        });
        let outlist_ok = j.outlist.iter().all(|o| {
            self.nodes.get(o).map_or(true, |n| n.truth_value == "OUT")
        });
        inlist_ok && outlist_ok
    }

    pub fn compute_truth(&self, node_id: &str) -> &str {
        match self.justifications.get(node_id) {
            None => self.nodes.get(node_id).map_or("OUT", |n| &n.truth_value),
            Some(justs) if justs.is_empty() => {
                self.nodes.get(node_id).map_or("OUT", |n| &n.truth_value)
            }
            Some(justs) => {
                if justs.iter().any(|j| self.justification_valid(j)) {
                    "IN"
                } else {
                    "OUT"
                }
            }
        }
    }

    pub fn propagate(&mut self, changed_id: &str) -> Vec<String> {
        let mut changed = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(changed_id.to_string());

        while let Some(current) = queue.pop_front() {
            if let Some(deps) = self.dependents.get(&current).cloned() {
                for dep_id in deps {
                    if let Some(node) = self.nodes.get(&dep_id) {
                        let is_retracted = node.metadata
                            .get("_retracted")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if is_retracted {
                            continue;
                        }
                    }

                    let new_truth = self.compute_truth(&dep_id).to_string();
                    if let Some(node) = self.nodes.get_mut(&dep_id) {
                        if node.truth_value != new_truth {
                            node.truth_value = new_truth;
                            changed.push(dep_id.clone());
                            queue.push_back(dep_id);
                        }
                    }
                }
            }
        }
        changed
    }

    pub fn retract(&mut self, node_id: &str, reason: Option<&str>) -> Vec<String> {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.truth_value = "OUT".to_string();
            if let Some(obj) = node.metadata.as_object_mut() {
                obj.insert("_retracted".to_string(), serde_json::json!(true));
                if let Some(r) = reason {
                    obj.insert("retract_reason".to_string(), serde_json::json!(r));
                }
            }
            node.retracted_at = chrono::Utc::now().to_rfc3339();
        }
        self.propagate(node_id)
    }

    pub fn assert_node(&mut self, node_id: &str) -> Vec<String> {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.truth_value = "IN".to_string();
            if let Some(obj) = node.metadata.as_object_mut() {
                obj.remove("_retracted");
                obj.remove("retract_reason");
            }
            node.retracted_at = String::new();
        }
        self.propagate(node_id)
    }

    pub fn add_justification(&mut self, node_id: &str, j: Justification) -> Vec<String> {
        for ant_id in &j.antecedents {
            self.dependents
                .entry(ant_id.clone())
                .or_default()
                .insert(node_id.to_string());
        }
        for out_id in &j.outlist {
            self.dependents
                .entry(out_id.clone())
                .or_default()
                .insert(node_id.to_string());
        }

        self.justifications
            .entry(node_id.to_string())
            .or_default()
            .push(j);

        let new_truth = self.compute_truth(node_id).to_string();
        let mut changed = Vec::new();
        if let Some(node) = self.nodes.get_mut(node_id) {
            if node.truth_value != new_truth {
                node.truth_value = new_truth;
                changed.push(node_id.to_string());
                changed.extend(self.propagate(node_id));
            }
        }
        changed
    }

    pub fn remove_justification(&mut self, node_id: &str, index: usize) -> Vec<String> {
        if let Some(justs) = self.justifications.get_mut(node_id) {
            if index < justs.len() {
                justs.remove(index);
            }
        }
        self.rebuild_dependents();

        let new_truth = self.compute_truth(node_id).to_string();
        let mut changed = Vec::new();
        if let Some(node) = self.nodes.get_mut(node_id) {
            if node.truth_value != new_truth {
                node.truth_value = new_truth;
                changed.push(node_id.to_string());
                changed.extend(self.propagate(node_id));
            }
        }
        changed
    }

    pub fn recompute_all(&mut self) -> Vec<String> {
        let max_iters = self.nodes.len() + 1;
        let mut all_changed = Vec::new();

        for _ in 0..max_iters {
            let mut changed_this_pass = false;
            let node_ids: Vec<String> = self.nodes.keys().cloned().collect();

            for node_id in &node_ids {
                let justs = self.justifications.get(node_id);
                let is_premise = justs.map_or(true, |v| v.is_empty());
                if is_premise {
                    continue;
                }

                let is_retracted = self.nodes.get(node_id)
                    .and_then(|n| n.metadata.get("_retracted"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if is_retracted {
                    continue;
                }

                let new_truth = self.compute_truth(node_id).to_string();
                if let Some(node) = self.nodes.get_mut(node_id) {
                    if node.truth_value != new_truth {
                        node.truth_value = new_truth;
                        all_changed.push(node_id.clone());
                        changed_this_pass = true;
                    }
                }
            }

            if !changed_this_pass {
                break;
            }
        }
        all_changed
    }

    pub fn entrenchment(&self, node_id: &str) -> i64 {
        let mut score: i64 = 0;
        let node = match self.nodes.get(node_id) {
            Some(n) => n,
            None => return 0,
        };

        let justs = self.justifications.get(node_id);
        let is_premise = justs.map_or(true, |v| v.is_empty());
        if is_premise {
            score += 100;
        }

        if !node.source.is_empty() {
            score += 50;
        }
        if !node.source_hash.is_empty() {
            score += 25;
        }

        let dep_count = self.dependents.get(node_id).map_or(0, |d| d.len());
        score += (dep_count as i64) * 10;

        let beliefs_type = node.metadata
            .get("beliefs_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        score += match beliefs_type {
            "AXIOM" | "WARNING" => 90,
            "OBSERVATION" => 80,
            "DERIVED" => 40,
            "PREDICTED" => 30,
            "NOTE" => 10,
            _ => 20,
        };

        score
    }

    pub fn find_premises(&self, node_id: &str) -> Vec<String> {
        let mut premises = Vec::new();
        let mut visited = HashSet::new();
        self.trace_premises(node_id, &mut premises, &mut visited);
        premises
    }

    fn trace_premises(&self, node_id: &str, premises: &mut Vec<String>, visited: &mut HashSet<String>) {
        if !visited.insert(node_id.to_string()) {
            return;
        }
        let justs = self.justifications.get(node_id);
        let is_premise = justs.map_or(true, |v| v.is_empty());
        if is_premise {
            premises.push(node_id.to_string());
            return;
        }
        if let Some(justs) = justs {
            for j in justs {
                for ant_id in &j.antecedents {
                    self.trace_premises(ant_id, premises, visited);
                }
            }
        }
    }

    pub fn find_culprits(&self, nogood_ids: &[String]) -> Vec<(String, i64)> {
        let mut all_premises = HashSet::new();
        for nid in nogood_ids {
            if let Some(node) = self.nodes.get(nid) {
                if node.truth_value == "IN" {
                    for p in self.find_premises(nid) {
                        all_premises.insert(p);
                    }
                }
            }
        }

        let mut candidates: Vec<(String, i64)> = all_premises
            .into_iter()
            .map(|p| {
                let score = self.entrenchment(&p);
                (p, score)
            })
            .collect();

        candidates.sort_by_key(|(_, score)| *score);
        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Node, Justification};

    fn premise(id: &str, text: &str) -> Node {
        Node::new(id.to_string(), text.to_string())
    }

    fn premise_out(id: &str, text: &str) -> Node {
        let mut n = Node::new(id.to_string(), text.to_string());
        n.truth_value = "OUT".to_string();
        n
    }

    fn sl(node_id: &str, ants: &[&str], outs: &[&str]) -> Justification {
        Justification::new_sl(
            node_id.to_string(),
            ants.iter().map(|s| s.to_string()).collect(),
            outs.iter().map(|s| s.to_string()).collect(),
            String::new(),
        )
    }

    fn build(nodes: Vec<Node>, justs: Vec<Justification>) -> Network {
        Network::load(nodes, justs)
    }

    // --- justification_valid ---

    #[test]
    fn justification_valid_all_in_antecedents() {
        let net = build(
            vec![premise("a", "A"), premise("b", "B")],
            vec![],
        );
        let j = sl("c", &["a", "b"], &[]);
        assert!(net.justification_valid(&j));
    }

    #[test]
    fn justification_valid_one_out_antecedent() {
        let net = build(
            vec![premise("a", "A"), premise_out("b", "B")],
            vec![],
        );
        let j = sl("c", &["a", "b"], &[]);
        assert!(!net.justification_valid(&j));
    }

    #[test]
    fn justification_valid_missing_antecedent_is_invalid() {
        let net = build(vec![premise("a", "A")], vec![]);
        let j = sl("c", &["a", "missing"], &[]);
        assert!(!net.justification_valid(&j));
    }

    #[test]
    fn justification_valid_outlist_in_is_invalid() {
        let net = build(
            vec![premise("a", "A"), premise("blocker", "B")],
            vec![],
        );
        let j = sl("c", &["a"], &["blocker"]);
        assert!(!net.justification_valid(&j));
    }

    #[test]
    fn justification_valid_outlist_out_is_valid() {
        let net = build(
            vec![premise("a", "A"), premise_out("blocker", "B")],
            vec![],
        );
        let j = sl("c", &["a"], &["blocker"]);
        assert!(net.justification_valid(&j));
    }

    #[test]
    fn justification_valid_missing_outlist_is_valid() {
        let net = build(vec![premise("a", "A")], vec![]);
        let j = sl("c", &["a"], &["nonexistent"]);
        assert!(net.justification_valid(&j));
    }

    // --- compute_truth ---

    #[test]
    fn compute_truth_premise_keeps_value() {
        let net = build(vec![premise("a", "A")], vec![]);
        assert_eq!(net.compute_truth("a"), "IN");

        let net = build(vec![premise_out("a", "A")], vec![]);
        assert_eq!(net.compute_truth("a"), "OUT");
    }

    #[test]
    fn compute_truth_derived_with_valid_justification() {
        let net = build(
            vec![premise("a", "A"), premise("b", "B")],
            vec![sl("b", &["a"], &[])],
        );
        assert_eq!(net.compute_truth("b"), "IN");
    }

    #[test]
    fn compute_truth_derived_with_no_valid_justification() {
        let net = build(
            vec![premise_out("a", "A"), premise("b", "B")],
            vec![sl("b", &["a"], &[])],
        );
        assert_eq!(net.compute_truth("b"), "OUT");
    }

    #[test]
    fn compute_truth_unknown_node() {
        let net = build(vec![], vec![]);
        assert_eq!(net.compute_truth("nonexistent"), "OUT");
    }

    // --- propagate ---

    #[test]
    fn propagate_single_level_cascade() {
        let mut net = build(
            vec![premise("a", "A"), premise("b", "B")],
            vec![sl("b", &["a"], &[])],
        );
        assert_eq!(net.nodes["b"].truth_value, "IN");

        net.nodes.get_mut("a").unwrap().truth_value = "OUT".to_string();
        let changed = net.propagate("a");
        assert_eq!(changed, vec!["b"]);
        assert_eq!(net.nodes["b"].truth_value, "OUT");
    }

    #[test]
    fn propagate_multi_level_cascade() {
        let mut net = build(
            vec![premise("a", "A"), premise("b", "B"), premise("c", "C")],
            vec![sl("b", &["a"], &[]), sl("c", &["b"], &[])],
        );

        net.nodes.get_mut("a").unwrap().truth_value = "OUT".to_string();
        let changed = net.propagate("a");
        assert!(changed.contains(&"b".to_string()));
        assert!(changed.contains(&"c".to_string()));
        assert_eq!(net.nodes["c"].truth_value, "OUT");
    }

    #[test]
    fn propagate_skips_retracted_nodes() {
        let mut b = premise("b", "B");
        if let Some(obj) = b.metadata.as_object_mut() {
            obj.insert("_retracted".to_string(), serde_json::json!(true));
        }
        b.truth_value = "OUT".to_string();

        let mut net = build(
            vec![premise("a", "A"), b],
            vec![sl("b", &["a"], &[])],
        );

        net.nodes.get_mut("a").unwrap().truth_value = "OUT".to_string();
        let changed = net.propagate("a");
        assert!(changed.is_empty());
    }

    // --- retract + assert round-trip ---

    #[test]
    fn retract_sets_out_and_metadata() {
        let mut net = build(vec![premise("a", "A")], vec![]);
        net.retract("a", Some("test reason"));

        let a = &net.nodes["a"];
        assert_eq!(a.truth_value, "OUT");
        assert_eq!(a.metadata["_retracted"], true);
        assert_eq!(a.metadata["retract_reason"], "test reason");
        assert!(!a.retracted_at.is_empty());
    }

    #[test]
    fn retract_cascades_to_dependents() {
        let mut net = build(
            vec![premise("a", "A"), premise("b", "B"), premise("c", "C")],
            vec![sl("b", &["a"], &[]), sl("c", &["b"], &[])],
        );
        let cascaded = net.retract("a", None);
        assert!(cascaded.contains(&"b".to_string()));
        assert!(cascaded.contains(&"c".to_string()));
        assert_eq!(net.nodes["b"].truth_value, "OUT");
        assert_eq!(net.nodes["c"].truth_value, "OUT");
    }

    #[test]
    fn assert_restores_and_cascades() {
        let mut net = build(
            vec![premise("a", "A"), premise("b", "B")],
            vec![sl("b", &["a"], &[])],
        );
        net.retract("a", None);
        assert_eq!(net.nodes["b"].truth_value, "OUT");

        let cascaded = net.assert_node("a");
        assert_eq!(net.nodes["a"].truth_value, "IN");
        assert!(net.nodes["a"].metadata.get("_retracted").is_none());
        assert!(cascaded.contains(&"b".to_string()));
        assert_eq!(net.nodes["b"].truth_value, "IN");
    }

    // --- add_justification ---

    #[test]
    fn add_justification_flips_out_to_in() {
        let mut net = build(
            vec![premise("a", "A"), premise_out("b", "B")],
            vec![],
        );
        assert_eq!(net.nodes["b"].truth_value, "OUT");

        let changed = net.add_justification("b", sl("b", &["a"], &[]));
        assert!(changed.contains(&"b".to_string()));
        assert_eq!(net.nodes["b"].truth_value, "IN");
    }

    #[test]
    fn add_invalid_justification_keeps_out() {
        let mut net = build(
            vec![premise_out("a", "A"), premise_out("b", "B")],
            vec![],
        );
        let changed = net.add_justification("b", sl("b", &["a"], &[]));
        assert!(changed.is_empty());
        assert_eq!(net.nodes["b"].truth_value, "OUT");
    }

    // --- remove_justification ---

    #[test]
    fn remove_last_justification_makes_premise() {
        // Removing the last justification makes a node a premise.
        // A premise keeps its current truth value (IN), so no change occurs.
        let mut net = build(
            vec![premise("a", "A"), premise("b", "B"), premise("c", "C")],
            vec![sl("b", &["a"], &[]), sl("c", &["b"], &[])],
        );
        let changed = net.remove_justification("b", 0);
        // b becomes a premise (keeps IN), c still depends on b which is IN
        assert!(changed.is_empty());
        assert_eq!(net.nodes["b"].truth_value, "IN");
        assert_eq!(net.nodes["c"].truth_value, "IN");
    }

    #[test]
    fn remove_justification_when_multiple() {
        // Node has two justifications; removing one doesn't change truth if other is valid
        let mut net = build(
            vec![premise("a", "A"), premise("b", "B"), premise("c", "C")],
            vec![sl("c", &["a"], &[]), sl("c", &["b"], &[])],
        );
        assert_eq!(net.nodes["c"].truth_value, "IN");

        let changed = net.remove_justification("c", 0);
        // Still IN via second justification
        assert!(changed.is_empty());
        assert_eq!(net.nodes["c"].truth_value, "IN");
    }

    // --- recompute_all ---

    #[test]
    fn recompute_all_reaches_fixpoint() {
        let mut net = build(
            vec![premise("a", "A"), premise("b", "B"), premise("c", "C")],
            vec![sl("b", &["a"], &[]), sl("c", &["b"], &[])],
        );
        // Manually set wrong truth values
        net.nodes.get_mut("b").unwrap().truth_value = "OUT".to_string();
        net.nodes.get_mut("c").unwrap().truth_value = "OUT".to_string();

        let changed = net.recompute_all();
        assert!(changed.contains(&"b".to_string()));
        assert!(changed.contains(&"c".to_string()));
        assert_eq!(net.nodes["b"].truth_value, "IN");
        assert_eq!(net.nodes["c"].truth_value, "IN");
    }

    #[test]
    fn recompute_all_no_changes_when_correct() {
        let mut net = build(
            vec![premise("a", "A"), premise("b", "B")],
            vec![sl("b", &["a"], &[])],
        );
        let changed = net.recompute_all();
        assert!(changed.is_empty());
    }

    // --- entrenchment ---

    #[test]
    fn entrenchment_premise_baseline() {
        let net = build(vec![premise("a", "A")], vec![]);
        // premise(100) + default type(20) = 120
        assert_eq!(net.entrenchment("a"), 120);
    }

    #[test]
    fn entrenchment_with_source() {
        let mut a = premise("a", "A");
        a.source = "doc.md".to_string();
        let net = build(vec![a], vec![]);
        // premise(100) + source(50) + default(20) = 170
        assert_eq!(net.entrenchment("a"), 170);
    }

    #[test]
    fn entrenchment_with_source_hash() {
        let mut a = premise("a", "A");
        a.source_hash = "abc123".to_string();
        let net = build(vec![a], vec![]);
        // premise(100) + hash(25) + default(20) = 145
        assert_eq!(net.entrenchment("a"), 145);
    }

    #[test]
    fn entrenchment_with_dependents() {
        let net = build(
            vec![premise("a", "A"), premise("b", "B")],
            vec![sl("b", &["a"], &[])],
        );
        // premise(100) + 1 dependent(10) + default(20) = 130
        assert_eq!(net.entrenchment("a"), 130);
    }

    #[test]
    fn entrenchment_beliefs_type_axiom() {
        let mut a = premise("a", "A");
        if let Some(obj) = a.metadata.as_object_mut() {
            obj.insert("beliefs_type".to_string(), serde_json::json!("AXIOM"));
        }
        let net = build(vec![a], vec![]);
        // premise(100) + axiom(90) = 190
        assert_eq!(net.entrenchment("a"), 190);
    }

    #[test]
    fn entrenchment_nonexistent_node() {
        let net = build(vec![], vec![]);
        assert_eq!(net.entrenchment("missing"), 0);
    }

    // --- find_premises ---

    #[test]
    fn find_premises_of_premise() {
        let net = build(vec![premise("a", "A")], vec![]);
        assert_eq!(net.find_premises("a"), vec!["a".to_string()]);
    }

    #[test]
    fn find_premises_through_chain() {
        let net = build(
            vec![premise("a", "A"), premise("b", "B"), premise("c", "C")],
            vec![sl("b", &["a"], &[]), sl("c", &["b"], &[])],
        );
        let premises = net.find_premises("c");
        assert_eq!(premises, vec!["a".to_string()]);
    }

    #[test]
    fn find_premises_handles_cycles() {
        // a depends on b, b depends on a — shouldn't infinite loop
        let net = build(
            vec![premise("a", "A"), premise("b", "B")],
            vec![sl("a", &["b"], &[]), sl("b", &["a"], &[])],
        );
        let premises = net.find_premises("a");
        assert!(premises.is_empty()); // neither is a premise
    }

    // --- find_culprits ---

    #[test]
    fn find_culprits_returns_least_entrenched_first() {
        let mut a = premise("a", "A");
        a.source = "doc.md".to_string(); // more entrenched
        let b = premise("b", "B"); // less entrenched

        let net = build(vec![a, b], vec![]);
        let culprits = net.find_culprits(&["a".to_string(), "b".to_string()]);

        assert_eq!(culprits.len(), 2);
        assert_eq!(culprits[0].0, "b"); // least entrenched first
        assert_eq!(culprits[1].0, "a");
    }

    // --- rebuild_dependents ---

    #[test]
    fn rebuild_dependents_indexes_antecedents_and_outlist() {
        let net = build(
            vec![premise("a", "A"), premise("b", "B"), premise("c", "C")],
            vec![sl("c", &["a"], &["b"])],
        );
        assert!(net.dependents["a"].contains("c"));
        assert!(net.dependents["b"].contains("c"));
    }
}


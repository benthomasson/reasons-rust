use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub text: String,
    pub truth_value: String,
    pub source: String,
    pub source_url: String,
    pub source_hash: String,
    pub date: String,
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
    pub reviewed_at: String,
    pub verified_at: String,
    pub retracted_at: String,
}

impl Node {
    pub fn new(id: String, text: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Node {
            id,
            text,
            truth_value: "IN".to_string(),
            source: String::new(),
            source_url: String::new(),
            source_hash: String::new(),
            date: String::new(),
            metadata: serde_json::json!({}),
            created_at: now.clone(),
            updated_at: now,
            reviewed_at: String::new(),
            verified_at: String::new(),
            retracted_at: String::new(),
        }
    }

    pub fn is_premise(&self, justification_count: usize) -> bool {
        justification_count == 0
    }

    pub fn beliefs_type(&self, justification_count: usize) -> &str {
        if let Some(bt) = self.metadata.get("beliefs_type").and_then(|v| v.as_str()) {
            return bt;
        }
        if justification_count > 0 {
            "DERIVED"
        } else {
            "OBSERVATION"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Justification {
    pub rowid: i64,
    pub node_id: String,
    pub jtype: String,
    pub antecedents: Vec<String>,
    pub outlist: Vec<String>,
    pub label: String,
}

impl Justification {
    pub fn new_sl(node_id: String, antecedents: Vec<String>, outlist: Vec<String>, label: String) -> Self {
        Justification {
            rowid: 0,
            node_id,
            jtype: "SL".to_string(),
            antecedents,
            outlist,
            label,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nogood {
    pub id: String,
    pub nodes: Vec<String>,
    pub discovered: String,
    pub resolution: String,
}

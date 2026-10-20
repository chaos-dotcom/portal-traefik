
//! Sync rules covering a destination's folder — port of `portal.services.sync_coverage`.
//! Pure and DB-only: never calls Frame.io.

use serde_json::{json, Value};
use sqlx::{PgPool, Row};

pub struct Rule {
    pub id: uuid::Uuid,
    pub name: String,
    pub destination_path: String,
    pub enabled: bool,
    pub source_config: Value,
}

/// (rule, exact) pairs. `exact` is false when covered by a recursive rule on an ancestor folder.
pub fn rules_covering<'a>(dest_config: &Value, rules: &'a [Rule]) -> Vec<(&'a Rule, bool)> {
    let account_id = dest_config.get("account_id").and_then(|v| v.as_str());
    let folder_id = dest_config.get("folder_id").and_then(|v| v.as_str());
    let (Some(account_id), Some(folder_id)) = (account_id, folder_id) else {
        return Vec::new();
    };
    let ancestors: Vec<String> = dest_config
        .get("path")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let upto = arr.len().saturating_sub(1);
            arr[..upto]
                .iter()
                .filter_map(|seg| seg.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut out = Vec::new();
    for rule in rules {
        let src = &rule.source_config;
        if src.get("account_id").and_then(|v| v.as_str()) != Some(account_id) {
            continue;
        }
        let rule_folder = src.get("folder_id").and_then(|v| v.as_str());
        if rule_folder == Some(folder_id) {
            out.push((rule, true));
        } else if src.get("recursive").map(|v| v.as_bool().unwrap_or(true)).unwrap_or(true)
            && let Some(rf) = rule_folder
                && ancestors.iter().any(|a| a == rf) {
                    out.push((rule, false));
                }
    }
    out
}

/// Every sync rule, for coverage checks.
pub async fn load_rules(db: &PgPool) -> Vec<Rule> {
    sqlx::query("SELECT id, name, destination_path, enabled, source_config FROM sync_rules")
        .fetch_all(db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| Rule {
            id: r.get("id"),
            name: r.get("name"),
            destination_path: r.get("destination_path"),
            enabled: r.get("enabled"),
            source_config: r.get("source_config"),
        })
        .collect()
}

/// The rules covering a folder, as the admin API renders them.
pub fn coverage_json(folder: &Value, rules: &[Rule]) -> Vec<Value> {
    rules_covering(folder, rules)
        .into_iter()
        .map(|(rule, exact)| {
            json!({
                "id": rule.id.to_string(),
                "name": rule.name,
                "destination_path": rule.destination_path,
                "enabled": rule.enabled,
                "recursive": rule.source_config.get("recursive").and_then(|v| v.as_bool()).unwrap_or(true),
                "exact": exact,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(folder: &str, recursive: bool) -> Rule {
        Rule {
            id: uuid::Uuid::nil(),
            name: format!("rule {folder}"),
            destination_path: "/nas".into(),
            enabled: true,
            source_config: json!({ "account_id": "a", "folder_id": folder, "recursive": recursive }),
        }
    }

    #[test]
    fn covers_the_folder_and_recursive_parents_only() {
        let folder = json!({ "account_id": "a", "folder_id": "day3",
            "path": [{ "id": "root", "name": "Northlight" }, { "id": "spring", "name": "Spring" }, { "id": "day3", "name": "Day 3" }] });
        let rules = vec![rule("day3", false), rule("spring", true), rule("root", false), rule("other", true)];
        let got: Vec<(String, bool)> = rules_covering(&folder, &rules).into_iter().map(|(r, e)| (r.name.clone(), e)).collect();
        assert_eq!(got, vec![("rule day3".to_string(), true), ("rule spring".to_string(), false)]);
    }
}

//! Utility functions for the MCP server
//!
//! This module contains helper functions used by tool handlers.

use crate::constants::DEFAULT_COLOR_ALPHA;
use serde_json::json;

/// Compute the difference between two UI trees
pub fn compute_tree_diff(
    tree_a: &egui_mcp_protocol::UiTree,
    tree_b: &egui_mcp_protocol::UiTree,
) -> serde_json::Value {
    use std::collections::HashMap;

    let map_a: HashMap<u64, &egui_mcp_protocol::NodeInfo> =
        tree_a.nodes.iter().map(|n| (n.id, n)).collect();
    let map_b: HashMap<u64, &egui_mcp_protocol::NodeInfo> =
        tree_b.nodes.iter().map(|n| (n.id, n)).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();

    // Find added nodes (in B but not in A)
    for (id, node) in &map_b {
        if !map_a.contains_key(id) {
            added.push(json!({
                "id": id,
                "role": node.role,
                "label": node.label
            }));
        }
    }

    // Find removed nodes (in A but not in B)
    for (id, node) in &map_a {
        if !map_b.contains_key(id) {
            removed.push(json!({
                "id": id,
                "role": node.role,
                "label": node.label
            }));
        }
    }

    // Find modified nodes (in both but different)
    for (id, node_a) in &map_a {
        if let Some(node_b) = map_b.get(id) {
            let mut changes = Vec::new();

            if node_a.role != node_b.role {
                changes.push(json!({
                    "field": "role",
                    "old": node_a.role,
                    "new": node_b.role
                }));
            }
            if node_a.label != node_b.label {
                changes.push(json!({
                    "field": "label",
                    "old": node_a.label,
                    "new": node_b.label
                }));
            }
            if node_a.value != node_b.value {
                changes.push(json!({
                    "field": "value",
                    "old": node_a.value,
                    "new": node_b.value
                }));
            }
            if node_a.toggled != node_b.toggled {
                changes.push(json!({
                    "field": "toggled",
                    "old": node_a.toggled,
                    "new": node_b.toggled
                }));
            }
            if node_a.disabled != node_b.disabled {
                changes.push(json!({
                    "field": "disabled",
                    "old": node_a.disabled,
                    "new": node_b.disabled
                }));
            }
            if node_a.focused != node_b.focused {
                changes.push(json!({
                    "field": "focused",
                    "old": node_a.focused,
                    "new": node_b.focused
                }));
            }

            if !changes.is_empty() {
                modified.push(json!({
                    "id": id,
                    "role": node_a.role,
                    "label": node_a.label,
                    "changes": changes
                }));
            }
        }
    }

    json!({
        "added_count": added.len(),
        "removed_count": removed.len(),
        "modified_count": modified.len(),
        "added": added,
        "removed": removed,
        "modified": modified
    })
}

/// Parse a hex color string to RGBA array
///
/// Supports formats:
/// - `#RRGGBB` - RGB with default alpha (DEFAULT_COLOR_ALPHA)
/// - `#RRGGBBAA` - RGBA
pub fn parse_hex_color(s: &str) -> Option<[u8; 4]> {
    let s = s.trim_start_matches('#');
    match s.len() {
        6 => {
            // #RRGGBB
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some([r, g, b, DEFAULT_COLOR_ALPHA])
        }
        8 => {
            // #RRGGBBAA
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            Some([r, g, b, a])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color_rgb() {
        assert_eq!(
            parse_hex_color("#ff0000"),
            Some([255, 0, 0, DEFAULT_COLOR_ALPHA])
        );
        assert_eq!(
            parse_hex_color("#00ff00"),
            Some([0, 255, 0, DEFAULT_COLOR_ALPHA])
        );
        assert_eq!(
            parse_hex_color("#0000ff"),
            Some([0, 0, 255, DEFAULT_COLOR_ALPHA])
        );
        assert_eq!(
            parse_hex_color("ff0000"),
            Some([255, 0, 0, DEFAULT_COLOR_ALPHA])
        );
    }

    #[test]
    fn test_parse_hex_color_rgba() {
        assert_eq!(parse_hex_color("#ff000080"), Some([255, 0, 0, 128]));
        assert_eq!(parse_hex_color("#00ff00ff"), Some([0, 255, 0, 255]));
    }

    #[test]
    fn test_parse_hex_color_invalid() {
        // 事前条件違反: 不正な長さ
        assert_eq!(parse_hex_color("#fff"), None);
        assert_eq!(parse_hex_color("#fffff"), None);
        assert_eq!(parse_hex_color(""), None);

        // 事前条件違反: 不正な文字
        assert_eq!(parse_hex_color("#gggggg"), None);
        assert_eq!(parse_hex_color("#zzzzzz"), None);
    }

    #[test]
    fn test_parse_hex_color_postconditions() {
        // 事後条件: 6文字の有効な入力 → alpha は必ず DEFAULT_COLOR_ALPHA
        let result = parse_hex_color("#123456").unwrap();
        assert_eq!(
            result[3], DEFAULT_COLOR_ALPHA,
            "6-char hex should have default alpha"
        );

        // 事後条件: 8文字の有効な入力 → alpha は入力通り
        let result = parse_hex_color("#12345678").unwrap();
        assert_eq!(result[3], 0x78, "8-char hex should preserve alpha");

        // 事後条件: 出力値は入力のパース結果と一致
        let result = parse_hex_color("#aabbcc").unwrap();
        assert_eq!(result[0], 0xaa);
        assert_eq!(result[1], 0xbb);
        assert_eq!(result[2], 0xcc);
    }

    #[test]
    fn test_compute_tree_diff_empty_trees() {
        let tree_a = egui_mcp_protocol::UiTree::default();
        let tree_b = egui_mcp_protocol::UiTree::default();
        let diff = compute_tree_diff(&tree_a, &tree_b);

        assert_eq!(diff["added_count"], 0);
        assert_eq!(diff["removed_count"], 0);
        assert_eq!(diff["modified_count"], 0);
    }

    #[test]
    fn test_compute_tree_diff_added_nodes() {
        let tree_a = egui_mcp_protocol::UiTree::default();
        let tree_b = egui_mcp_protocol::UiTree {
            roots: vec![1],
            nodes: vec![egui_mcp_protocol::NodeInfo {
                id: 1,
                role: "Button".to_string(),
                label: Some("Click me".to_string()),
                value: None,
                bounds: None,
                children: vec![],
                toggled: None,
                disabled: false,
                focused: false,
            }],
        };
        let diff = compute_tree_diff(&tree_a, &tree_b);

        assert_eq!(diff["added_count"], 1);
        assert_eq!(diff["removed_count"], 0);
        assert_eq!(diff["modified_count"], 0);
        assert_eq!(diff["added"][0]["id"], 1);
        assert_eq!(diff["added"][0]["role"], "Button");
    }

    #[test]
    fn test_compute_tree_diff_removed_nodes() {
        let tree_a = egui_mcp_protocol::UiTree {
            roots: vec![1],
            nodes: vec![egui_mcp_protocol::NodeInfo {
                id: 1,
                role: "Label".to_string(),
                label: Some("Hello".to_string()),
                value: None,
                bounds: None,
                children: vec![],
                toggled: None,
                disabled: false,
                focused: false,
            }],
        };
        let tree_b = egui_mcp_protocol::UiTree::default();
        let diff = compute_tree_diff(&tree_a, &tree_b);

        assert_eq!(diff["added_count"], 0);
        assert_eq!(diff["removed_count"], 1);
        assert_eq!(diff["modified_count"], 0);
        assert_eq!(diff["removed"][0]["id"], 1);
    }

    #[test]
    fn test_compute_tree_diff_modified_nodes() {
        let tree_a = egui_mcp_protocol::UiTree {
            roots: vec![1],
            nodes: vec![egui_mcp_protocol::NodeInfo {
                id: 1,
                role: "TextInput".to_string(),
                label: Some("Name".to_string()),
                value: Some("old".to_string()),
                bounds: None,
                children: vec![],
                toggled: None,
                disabled: false,
                focused: false,
            }],
        };
        let tree_b = egui_mcp_protocol::UiTree {
            roots: vec![1],
            nodes: vec![egui_mcp_protocol::NodeInfo {
                id: 1,
                role: "TextInput".to_string(),
                label: Some("Name".to_string()),
                value: Some("new".to_string()),
                bounds: None,
                children: vec![],
                toggled: None,
                disabled: true,
                focused: true,
            }],
        };
        let diff = compute_tree_diff(&tree_a, &tree_b);

        assert_eq!(diff["added_count"], 0);
        assert_eq!(diff["removed_count"], 0);
        assert_eq!(diff["modified_count"], 1);

        let changes = &diff["modified"][0]["changes"];
        assert!(
            changes
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["field"] == "value")
        );
        assert!(
            changes
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["field"] == "disabled")
        );
        assert!(
            changes
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["field"] == "focused")
        );
    }

    #[test]
    fn test_compute_tree_diff_postconditions() {
        // セットアップ: 複合的なケース
        let tree_a = egui_mcp_protocol::UiTree {
            roots: vec![1, 2],
            nodes: vec![
                egui_mcp_protocol::NodeInfo {
                    id: 1,
                    role: "Button".to_string(),
                    label: Some("A".to_string()),
                    value: None,
                    bounds: None,
                    children: vec![],
                    toggled: None,
                    disabled: false,
                    focused: false,
                },
                egui_mcp_protocol::NodeInfo {
                    id: 2,
                    role: "Label".to_string(),
                    label: Some("B".to_string()),
                    value: Some("old".to_string()),
                    bounds: None,
                    children: vec![],
                    toggled: None,
                    disabled: false,
                    focused: false,
                },
            ],
        };
        let tree_b = egui_mcp_protocol::UiTree {
            roots: vec![2, 3],
            nodes: vec![
                egui_mcp_protocol::NodeInfo {
                    id: 2,
                    role: "Label".to_string(),
                    label: Some("B".to_string()),
                    value: Some("new".to_string()), // modified
                    bounds: None,
                    children: vec![],
                    toggled: None,
                    disabled: false,
                    focused: false,
                },
                egui_mcp_protocol::NodeInfo {
                    id: 3,
                    role: "TextInput".to_string(),
                    label: Some("C".to_string()),
                    value: None,
                    bounds: None,
                    children: vec![],
                    toggled: None,
                    disabled: false,
                    focused: false,
                },
            ],
        };

        let diff = compute_tree_diff(&tree_a, &tree_b);

        // 事後条件1: added のIDは tree_b にあって tree_a にない
        let ids_a: std::collections::HashSet<u64> = tree_a.nodes.iter().map(|n| n.id).collect();
        let ids_b: std::collections::HashSet<u64> = tree_b.nodes.iter().map(|n| n.id).collect();

        for added in diff["added"].as_array().unwrap() {
            let id = added["id"].as_u64().unwrap();
            assert!(ids_b.contains(&id), "added id {} should be in tree_b", id);
            assert!(
                !ids_a.contains(&id),
                "added id {} should not be in tree_a",
                id
            );
        }

        // 事後条件2: removed のIDは tree_a にあって tree_b にない
        for removed in diff["removed"].as_array().unwrap() {
            let id = removed["id"].as_u64().unwrap();
            assert!(ids_a.contains(&id), "removed id {} should be in tree_a", id);
            assert!(
                !ids_b.contains(&id),
                "removed id {} should not be in tree_b",
                id
            );
        }

        // 事後条件3: modified のIDは両方に存在する
        for modified in diff["modified"].as_array().unwrap() {
            let id = modified["id"].as_u64().unwrap();
            assert!(
                ids_a.contains(&id),
                "modified id {} should be in tree_a",
                id
            );
            assert!(
                ids_b.contains(&id),
                "modified id {} should be in tree_b",
                id
            );
        }

        // 事後条件4: カウントと配列長が一致
        assert_eq!(
            diff["added_count"].as_u64().unwrap() as usize,
            diff["added"].as_array().unwrap().len()
        );
        assert_eq!(
            diff["removed_count"].as_u64().unwrap() as usize,
            diff["removed"].as_array().unwrap().len()
        );
        assert_eq!(
            diff["modified_count"].as_u64().unwrap() as usize,
            diff["modified"].as_array().unwrap().len()
        );
    }
}

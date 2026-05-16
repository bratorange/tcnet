use egui_mcp_protocol::{NodeInfo, Rect, UiTree};

pub fn convert(tree: &accesskit::TreeUpdate) -> UiTree {
    let focus_id = tree.focus.0;
    let root_id = tree.tree.as_ref().map(|t| t.root.0);

    let nodes: Vec<NodeInfo> = tree
        .nodes
        .iter()
        .map(|(node_id, node)| {
            let id = node_id.0;

            let bounds = node.bounds().map(|r| Rect {
                x: r.x0 as f32,
                y: r.y0 as f32,
                width: (r.x1 - r.x0) as f32,
                height: (r.y1 - r.y0) as f32,
            });

            let toggled = node.toggled().map(|t| match t {
                accesskit::Toggled::True => true,
                accesskit::Toggled::False | accesskit::Toggled::Mixed => false,
            });

            NodeInfo {
                id,
                role: format!("{:?}", node.role()),
                label: node.label().map(String::from),
                value: node.value().map(String::from),
                bounds,
                children: node.children().iter().map(|c| c.0).collect(),
                toggled,
                disabled: node.is_disabled(),
                focused: id == focus_id,
            }
        })
        .collect();

    let roots = root_id
        .map(|r| vec![r])
        .unwrap_or_else(|| nodes.first().map(|n| vec![n.id]).unwrap_or_default());

    UiTree { roots, nodes }
}

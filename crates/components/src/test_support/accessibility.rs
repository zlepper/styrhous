use super::*;

pub(super) fn format_node(
    output: &mut String,
    node: &Node<'_>,
    pixels_per_point: f32,
    depth: usize,
    is_root: bool,
    options: &AccessibilityTreeOptions,
) {
    let accesskit_node = node.accesskit_node();
    let include = is_root
        || options.include_structural_nodes
        || accesskit_node.role() != Role::GenericContainer
        || accesskit_node.label().is_some();
    let child_depth = depth + usize::from(include);

    if include {
        output.push_str(&"  ".repeat(depth));
        let _ = write!(output, "{:?}", accesskit_node.role());
        if let Some(name) = accesskit_node.label() {
            let _ = write!(output, " name={name:?}");
        }
        if let Some(value) = accesskit_node.value() {
            let _ = write!(output, " value={value:?}");
        }

        let mut states = Vec::new();
        if accesskit_node.is_focused() {
            states.push("focused".to_owned());
        }
        if accesskit_node.is_disabled() {
            states.push("disabled".to_owned());
        }
        if accesskit_node.is_hidden() {
            states.push("hidden".to_owned());
        }
        if accesskit_node.is_selected() == Some(true) {
            states.push("selected".to_owned());
        }
        if let Some(toggled) = accesskit_node.toggled() {
            states.push(format!("toggled={toggled:?}"));
        }
        if !states.is_empty() {
            let _ = write!(output, " state=[{}]", states.join(", "));
        }

        match accesskit_node.bounding_box() {
            Some(rect) => {
                let x = rect.x0 as f32 / pixels_per_point;
                let y = rect.y0 as f32 / pixels_per_point;
                let width = (rect.x1 - rect.x0) as f32 / pixels_per_point;
                let height = (rect.y1 - rect.y0) as f32 / pixels_per_point;
                let center_x = x + width / 2.0;
                let center_y = y + height / 2.0;
                let _ = write!(
                    output,
                    " rect=(x={} y={} width={} height={} center_x={} center_y={})",
                    coordinate(x),
                    coordinate(y),
                    coordinate(width),
                    coordinate(height),
                    coordinate(center_x),
                    coordinate(center_y),
                );
            }
            None => output.push_str(" rect=<none>"),
        }
        output.push('\n');
    }

    for child in node.children() {
        format_node(
            output,
            &child,
            pixels_per_point,
            child_depth,
            false,
            options,
        );
    }
}

const MINIMUM_OVERLAP_SIZE: f32 = 1.0;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct AccessibilityNodeDescription {
    pub(super) role: String,
    pub(super) name: Option<String>,
    pub(super) value: Option<String>,
    pub(super) rect: Rect,
}

impl Display for AccessibilityNodeDescription {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.role)?;
        if let Some(name) = &self.name {
            write!(formatter, " name={name:?}")?;
        }
        if let Some(value) = &self.value {
            write!(formatter, " value={value:?}")?;
        }
        write!(formatter, " {}", format_rect(self.rect))
    }
}

#[derive(Clone, Debug)]
pub(super) struct AccessibilityNodeInfo {
    description: AccessibilityNodeDescription,
    actions: Vec<Action>,
    child_count: usize,
    parent: Option<usize>,
    layer: Option<usize>,
    hidden: bool,
}

fn collect_accessibility_nodes(
    root: &Node<'_>,
    pixels_per_point: f32,
    viewport: Rect,
    nodes: &mut Vec<AccessibilityNodeInfo>,
) {
    let root_index =
        collect_accessibility_node(root, pixels_per_point, viewport, None, None, nodes);
    for (layer, child) in root.children().enumerate() {
        collect_accessibility_branch(
            &child,
            pixels_per_point,
            viewport,
            Some(root_index),
            Some(layer),
            nodes,
        );
    }
}

fn collect_accessibility_branch(
    node: &Node<'_>,
    pixels_per_point: f32,
    visible_rect: Rect,
    parent: Option<usize>,
    layer: Option<usize>,
    nodes: &mut Vec<AccessibilityNodeInfo>,
) {
    let index =
        collect_accessibility_node(node, pixels_per_point, visible_rect, parent, layer, nodes);
    let child_visible_rect = clip_rect_for_scrollbars(node, pixels_per_point, visible_rect);
    for child in node.children() {
        collect_accessibility_branch(
            &child,
            pixels_per_point,
            child_visible_rect,
            Some(index),
            layer,
            nodes,
        );
    }
}

fn collect_accessibility_node(
    node: &Node<'_>,
    pixels_per_point: f32,
    visible_rect: Rect,
    parent: Option<usize>,
    layer: Option<usize>,
    nodes: &mut Vec<AccessibilityNodeInfo>,
) -> usize {
    let accesskit_node = node.accesskit_node();
    let child_count = node.children().count();
    let rect = accesskit_rect(accesskit_node.bounding_box(), pixels_per_point)
        .map(|rect| rect.intersect(visible_rect));
    let index = nodes.len();
    if let Some(rect) = rect {
        nodes.push(AccessibilityNodeInfo {
            description: AccessibilityNodeDescription {
                role: format!("{:?}", accesskit_node.role()),
                name: accesskit_node.label(),
                value: accesskit_node.value(),
                rect,
            },
            actions: label_required_actions(&accesskit_node),
            child_count,
            parent,
            layer,
            hidden: accesskit_node.is_hidden() || !rect.is_positive(),
        });
    } else {
        nodes.push(AccessibilityNodeInfo {
            description: AccessibilityNodeDescription {
                role: format!("{:?}", accesskit_node.role()),
                name: accesskit_node.label(),
                value: accesskit_node.value(),
                rect: Rect::NOTHING,
            },
            actions: label_required_actions(&accesskit_node),
            child_count,
            parent,
            layer,
            hidden: true,
        });
    }

    index
}

fn label_required_actions(node: &egui_kittest::kittest::AccessKitNode<'_>) -> Vec<Action> {
    const ACTIONS: [Action; 11] = [
        Action::Click,
        Action::Focus,
        Action::Collapse,
        Action::Expand,
        Action::CustomAction,
        Action::Decrement,
        Action::Increment,
        Action::ReplaceSelectedText,
        Action::SetTextSelection,
        Action::SetValue,
        Action::ShowContextMenu,
    ];

    ACTIONS
        .iter()
        .copied()
        .filter(|action| node.data().supports_action(*action))
        .collect()
}

pub(super) fn current_accessibility_nodes<State>(
    harness: &Harness<'_, State>,
) -> Vec<AccessibilityNodeInfo> {
    let mut nodes = Vec::new();
    collect_accessibility_nodes(
        &harness.root(),
        harness.ctx.pixels_per_point(),
        harness.ctx.viewport_rect(),
        &mut nodes,
    );
    nodes
}

fn clip_rect_for_scrollbars(
    node: &Node<'_>,
    pixels_per_point: f32,
    mut visible_rect: Rect,
) -> Rect {
    for child in node.children() {
        let child_node = child.accesskit_node();
        if child_node.role() != Role::ScrollBar {
            continue;
        }
        let Some(scrollbar_rect) = accesskit_rect(child_node.bounding_box(), pixels_per_point)
        else {
            continue;
        };

        if scrollbar_rect.height() > scrollbar_rect.width() {
            visible_rect.min.y = visible_rect.min.y.max(scrollbar_rect.min.y);
            visible_rect.max.y = visible_rect.max.y.min(scrollbar_rect.max.y);
        } else {
            visible_rect.min.x = visible_rect.min.x.max(scrollbar_rect.min.x);
            visible_rect.max.x = visible_rect.max.x.min(scrollbar_rect.max.x);
        }
    }
    visible_rect
}

fn accesskit_rect(rect: Option<egui::accesskit::Rect>, pixels_per_point: f32) -> Option<Rect> {
    rect.map(|rect| {
        Rect::from_min_max(
            Pos2::new(
                rect.x0 as f32 / pixels_per_point,
                rect.y0 as f32 / pixels_per_point,
            ),
            Pos2::new(
                rect.x1 as f32 / pixels_per_point,
                rect.y1 as f32 / pixels_per_point,
            ),
        )
    })
}

pub(super) fn find_illegal_overlaps(nodes: &[AccessibilityNodeInfo]) -> Vec<AccessibilityOverlap> {
    let mut overlaps = Vec::new();
    for (first_index, first) in nodes.iter().enumerate() {
        if !is_overlap_candidate(first_index, nodes) {
            continue;
        }
        for (second_index, second) in nodes.iter().enumerate().skip(first_index + 1) {
            if !is_overlap_candidate(second_index, nodes)
                || first.layer != second.layer
                || nodes_are_related(first_index, second_index, nodes)
                || is_composite_control_content(first, second)
                || is_composite_control_content(second, first)
            {
                continue;
            }

            let intersection = first.description.rect.intersect(second.description.rect);
            if intersection.width() >= MINIMUM_OVERLAP_SIZE
                && intersection.height() >= MINIMUM_OVERLAP_SIZE
            {
                overlaps.push(AccessibilityOverlap {
                    first: first.description.clone(),
                    second: second.description.clone(),
                    intersection,
                });
            }
        }
    }
    overlaps
}

pub(super) fn find_unlabeled_interactive_nodes(
    nodes: &[AccessibilityNodeInfo],
) -> Vec<AccessibilityLabelViolation> {
    nodes
        .iter()
        .filter(|node| {
            !node.hidden
                && !node.actions.is_empty()
                // egui labels expose Click for text selection even when they are not controls.
                && !matches!(
                    node.description.role.as_str(),
                    "Label" | "ScrollBar" | "TextRun"
                )
                // egui represents structural surfaces such as menus, tooltips, and scroll
                // containers as action-bearing Unknown/GenericContainer parents. Their child
                // controls are checked independently; a leaf of either role comes from a
                // direct `ui.interact` and must itself have a name.
                && (!matches!(
                    node.description.role.as_str(),
                    "GenericContainer" | "Unknown"
                ) || node.child_count == 0)
                && node
                    .description
                    .name
                    .as_deref()
                    .is_none_or(|name| name.trim().is_empty())
        })
        .map(|node| AccessibilityLabelViolation {
            description: node.description.clone(),
            actions: node.actions.clone(),
        })
        .collect()
}

fn is_overlap_candidate(index: usize, nodes: &[AccessibilityNodeInfo]) -> bool {
    let node = &nodes[index];
    !node.hidden
        && node.description.rect.is_positive()
        && !matches!(
            node.description.role.as_str(),
            "Window" | "Unknown" | "GenericContainer" | "Image" | "ScrollBar"
        )
        && (node.description.role != "Label" || has_descendant_role(index, "TextRun", nodes))
}

fn is_composite_control_content(
    outer: &AccessibilityNodeInfo,
    inner: &AccessibilityNodeInfo,
) -> bool {
    let contains_inner = outer.description.rect.contains_rect(inner.description.rect);
    (outer.description.role == "ComboBox"
        && matches!(inner.description.role.as_str(), "TextInput" | "TextRun")
        && contains_inner)
        || (outer.description.role == "Button"
            && matches!(inner.description.role.as_str(), "Label" | "TextRun")
            && contains_inner
            && inner
                .description
                .name
                .as_deref()
                .or(inner.description.value.as_deref())
                .is_some_and(|text| {
                    outer
                        .description
                        .name
                        .as_deref()
                        .is_some_and(|name| name.contains(text))
                }))
}

fn has_descendant_role(index: usize, role: &str, nodes: &[AccessibilityNodeInfo]) -> bool {
    nodes.iter().enumerate().any(|(candidate, node)| {
        node.description.role == role && is_ancestor(index, candidate, nodes)
    })
}

fn nodes_are_related(first: usize, second: usize, nodes: &[AccessibilityNodeInfo]) -> bool {
    is_ancestor(first, second, nodes) || is_ancestor(second, first, nodes)
}

fn is_ancestor(ancestor: usize, mut descendant: usize, nodes: &[AccessibilityNodeInfo]) -> bool {
    while let Some(parent) = nodes[descendant].parent {
        if parent == ancestor {
            return true;
        }
        descendant = parent;
    }
    false
}

pub(super) fn illegal_overlaps_message(overlaps: &[AccessibilityOverlap]) -> String {
    let overlaps = overlaps
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    format!("Illegal accessibility overlaps:\n{overlaps}")
}

pub(super) fn missing_labels_message(labels: &[AccessibilityLabelViolation]) -> String {
    let labels = labels
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    format!("Interactive accessibility nodes without labels:\n{labels}")
}

pub(super) fn format_rect(rect: Rect) -> String {
    let width = rect.width();
    let height = rect.height();
    format!(
        "rect=(x={} y={} width={} height={} center_x={} center_y={})",
        coordinate(rect.min.x),
        coordinate(rect.min.y),
        coordinate(width),
        coordinate(height),
        coordinate(rect.center().x),
        coordinate(rect.center().y),
    )
}

pub(super) fn coordinate(value: f32) -> String {
    let value = if value.abs() < f32::EPSILON {
        0.0
    } else {
        value
    };
    format!("{value:.1}")
}

use vello::{
    Scene,
    peniko::{Fill, color::{AlphaColor, Srgb}},
    kurbo::{Affine, Rect, RoundedRect},
};

use crate::node::{NodeKind, SceneGraph};

pub fn build_scene(graph: &SceneGraph) -> Scene {
    let mut scene = Scene::new();
    for (_, node) in graph.iter() {
        match node.kind {
            NodeKind::Rect { x, y, width, height, color, corner_radius } => {
                let brush = AlphaColor::<Srgb>::new(color);
                let x0 = x as f64;
                let y0 = y as f64;
                let x1 = (x + width) as f64;
                let y1 = (y + height) as f64;
                if corner_radius == 0.0 {
                    let rect = Rect::new(x0, y0, x1, y1);
                    scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &rect);
                } else {
                    let rect = RoundedRect::new(x0, y0, x1, y1, corner_radius as f64);
                    scene.fill(Fill::NonZero, Affine::IDENTITY, brush, None, &rect);
                }
            }
        }
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{Node, NodeKind, SceneGraph};

    fn rect_node(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Node {
        Node {
            kind: NodeKind::Rect {
                x,
                y,
                width: w,
                height: h,
                color: [1.0, 0.0, 0.0, 1.0],
                corner_radius: radius,
            },
            children: vec![],
            parent: None,
        }
    }

    #[test]
    fn empty_graph_builds_without_panic() {
        let sg = SceneGraph::new();
        let _ = build_scene(&sg);
    }

    #[test]
    fn single_rect_builds_without_panic() {
        let mut sg = SceneGraph::new();
        sg.insert(rect_node(0.0, 0.0, 100.0, 100.0, 0.0));
        let _ = build_scene(&sg);
    }

    #[test]
    fn rounded_rect_builds_without_panic() {
        let mut sg = SceneGraph::new();
        sg.insert(rect_node(10.0, 10.0, 80.0, 80.0, 12.0));
        let _ = build_scene(&sg);
    }

    #[test]
    fn multiple_rects_build_without_panic() {
        let mut sg = SceneGraph::new();
        sg.insert(rect_node(0.0, 0.0, 100.0, 100.0, 0.0));
        sg.insert(rect_node(150.0, 50.0, 200.0, 120.0, 8.0));
        sg.insert(rect_node(400.0, 200.0, 60.0, 60.0, 30.0));
        let _ = build_scene(&sg);
    }
}

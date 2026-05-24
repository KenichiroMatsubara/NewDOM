use slotmap::{DefaultKey, SlotMap};

pub type NodeId = DefaultKey;

#[derive(Debug, Clone)]
pub enum NodeKind {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        corner_radius: f32,
    },
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
}

pub struct SceneGraph {
    nodes: SlotMap<NodeId, Node>,
    root: Option<NodeId>,
}

impl SceneGraph {
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::new(),
            root: None,
        }
    }

    pub fn insert(&mut self, node: Node) -> NodeId {
        let id = self.nodes.insert(node);
        if self.root.is_none() {
            self.root = Some(id);
        }
        id
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(id)
    }

    /// Update the NodeKind of an existing node. Returns false if the ID is stale.
    pub fn update_kind(&mut self, id: NodeId, kind: NodeKind) -> bool {
        match self.nodes.get_mut(id) {
            Some(node) => {
                node.kind = kind;
                true
            }
            None => false,
        }
    }

    /// Set the parent of `child` to `parent`. Returns false if either ID is stale.
    pub fn set_parent(&mut self, child: NodeId, parent: NodeId) -> bool {
        if !self.nodes.contains_key(child) || !self.nodes.contains_key(parent) {
            return false;
        }
        let old_parent_id = self.nodes[child].parent;
        if let Some(old_pid) = old_parent_id {
            if let Some(op) = self.nodes.get_mut(old_pid) {
                op.children.retain(|&c| c != child);
            }
        }
        self.nodes[child].parent = Some(parent);
        self.nodes[parent].children.push(child);
        true
    }

    pub fn remove(&mut self, id: NodeId) -> Option<Node> {
        let node = self.nodes.remove(id)?;
        if let Some(parent_id) = node.parent {
            if let Some(parent) = self.nodes.get_mut(parent_id) {
                parent.children.retain(|&c| c != id);
            }
        }
        if self.root == Some(id) {
            self.root = None;
        }
        Some(node)
    }

    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter()
    }
}

impl Default for SceneGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rect(x: f32, y: f32, w: f32, h: f32) -> Node {
        Node {
            kind: NodeKind::Rect {
                x,
                y,
                width: w,
                height: h,
                color: [1.0, 0.0, 0.0, 1.0],
                corner_radius: 0.0,
            },
            children: Vec::new(),
            parent: None,
        }
    }

    #[test]
    fn insert_and_get() {
        let mut sg = SceneGraph::new();
        let id = sg.insert(make_rect(0.0, 0.0, 100.0, 100.0));
        assert!(sg.get(id).is_some());
    }

    #[test]
    fn first_insert_becomes_root() {
        let mut sg = SceneGraph::new();
        let id = sg.insert(make_rect(0.0, 0.0, 10.0, 10.0));
        assert_eq!(sg.root(), Some(id));
    }

    #[test]
    fn second_insert_does_not_change_root() {
        let mut sg = SceneGraph::new();
        let a = sg.insert(make_rect(0.0, 0.0, 10.0, 10.0));
        let _b = sg.insert(make_rect(5.0, 5.0, 10.0, 10.0));
        assert_eq!(sg.root(), Some(a));
    }

    #[test]
    fn remove_node() {
        let mut sg = SceneGraph::new();
        let id = sg.insert(make_rect(0.0, 0.0, 10.0, 10.0));
        sg.remove(id);
        assert!(sg.get(id).is_none());
    }

    #[test]
    fn remove_root_clears_root() {
        let mut sg = SceneGraph::new();
        let id = sg.insert(make_rect(0.0, 0.0, 10.0, 10.0));
        sg.remove(id);
        assert_eq!(sg.root(), None);
    }

    #[test]
    fn contains_returns_false_for_removed_node() {
        let mut sg = SceneGraph::new();
        let id = sg.insert(make_rect(0.0, 0.0, 10.0, 10.0));
        sg.remove(id);
        assert!(!sg.contains(id));
    }

    #[test]
    fn stale_id_update_returns_false() {
        let mut sg = SceneGraph::new();
        let id = sg.insert(make_rect(0.0, 0.0, 10.0, 10.0));
        sg.remove(id);
        let updated = sg.update_kind(
            id,
            NodeKind::Rect {
                x: 5.0,
                y: 5.0,
                width: 50.0,
                height: 50.0,
                color: [0.0, 1.0, 0.0, 1.0],
                corner_radius: 0.0,
            },
        );
        assert!(!updated);
    }

    #[test]
    fn update_kind_modifies_existing_node() {
        let mut sg = SceneGraph::new();
        let id = sg.insert(make_rect(0.0, 0.0, 10.0, 10.0));
        let ok = sg.update_kind(
            id,
            NodeKind::Rect {
                x: 99.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                color: [1.0, 0.0, 0.0, 1.0],
                corner_radius: 0.0,
            },
        );
        assert!(ok);
        let NodeKind::Rect { x, .. } = sg.get(id).unwrap().kind;
        assert_eq!(x, 99.0);
    }

    #[test]
    fn set_parent_establishes_relationship() {
        let mut sg = SceneGraph::new();
        let parent = sg.insert(make_rect(0.0, 0.0, 200.0, 200.0));
        let child = sg.insert(make_rect(10.0, 10.0, 50.0, 50.0));
        assert!(sg.set_parent(child, parent));
        assert_eq!(sg.get(child).unwrap().parent, Some(parent));
        assert!(sg.get(parent).unwrap().children.contains(&child));
    }

    #[test]
    fn set_parent_stale_child_returns_false() {
        let mut sg = SceneGraph::new();
        let parent = sg.insert(make_rect(0.0, 0.0, 200.0, 200.0));
        let child = sg.insert(make_rect(10.0, 10.0, 50.0, 50.0));
        sg.remove(child);
        assert!(!sg.set_parent(child, parent));
    }

    #[test]
    fn set_parent_stale_parent_returns_false() {
        let mut sg = SceneGraph::new();
        let parent = sg.insert(make_rect(0.0, 0.0, 200.0, 200.0));
        let child = sg.insert(make_rect(10.0, 10.0, 50.0, 50.0));
        sg.remove(parent);
        assert!(!sg.set_parent(child, parent));
    }

    #[test]
    fn remove_cleans_parent_children_list() {
        let mut sg = SceneGraph::new();
        let parent = sg.insert(make_rect(0.0, 0.0, 200.0, 200.0));
        let child = sg.insert(make_rect(10.0, 10.0, 50.0, 50.0));
        sg.set_parent(child, parent);
        sg.remove(child);
        assert!(sg.get(parent).unwrap().children.is_empty());
    }

    #[test]
    fn reparent_removes_from_old_parent() {
        let mut sg = SceneGraph::new();
        let p1 = sg.insert(make_rect(0.0, 0.0, 100.0, 100.0));
        let p2 = sg.insert(make_rect(200.0, 0.0, 100.0, 100.0));
        let child = sg.insert(make_rect(10.0, 10.0, 20.0, 20.0));
        sg.set_parent(child, p1);
        sg.set_parent(child, p2);
        assert!(!sg.get(p1).unwrap().children.contains(&child));
        assert!(sg.get(p2).unwrap().children.contains(&child));
        assert_eq!(sg.get(child).unwrap().parent, Some(p2));
    }
}

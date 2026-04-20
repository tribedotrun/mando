//! Minimal DOM tree for html5ever 0.29.
//!
//! Implements `TreeSink` so `parse_document` can build a tree we own.
//! Nodes are arena-allocated via indices into a `Vec<Node>`.

use std::borrow::Cow;
use std::cell::RefCell;

use html5ever::tendril::StrTendril;
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, QualName};

// ---------------------------------------------------------------------------
// Node types
// ---------------------------------------------------------------------------

/// Index into `Dom::nodes`.
pub(crate) type NodeId = usize;

/// The kind of a DOM node.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields required by TreeSink but not read externally
pub(crate) enum NodeKind {
    Document,
    Element {
        name: QualName,
        attrs: Vec<Attribute>,
        /// True if this is a `<template>` element.
        template_contents: Option<NodeId>,
    },
    Text(String),
    Comment(String),
    Doctype {
        name: String,
        public_id: String,
        system_id: String,
    },
    ProcessingInstruction {
        target: String,
        data: String,
    },
}

/// A single DOM node.
#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

// ---------------------------------------------------------------------------
// Dom — the arena + TreeSink
// ---------------------------------------------------------------------------

/// Arena-based DOM.  All nodes live in `self.nodes`.
pub(crate) struct Dom {
    nodes: RefCell<Vec<Node>>,
}

impl Dom {
    pub(crate) fn new() -> Self {
        let root = Node {
            kind: NodeKind::Document,
            parent: None,
            children: Vec::new(),
        };
        Dom {
            nodes: RefCell::new(vec![root]),
        }
    }

    /// The document root is always node 0.
    pub(crate) fn document_id(&self) -> NodeId {
        0
    }

    /// Read-only access to node data.
    pub(crate) fn node(&self, id: NodeId) -> Node {
        self.nodes.borrow()[id].clone()
    }

    /// Children of a node.
    pub(crate) fn children(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes.borrow()[id].children.clone()
    }

    /// Get the local tag name of an element, lowercased.
    /// Returns `None` for non-element nodes.
    pub(crate) fn tag_name(&self, id: NodeId) -> Option<String> {
        let nodes = self.nodes.borrow();
        match &nodes[id].kind {
            NodeKind::Element { name, .. } => Some(name.local.to_string()),
            _ => None,
        }
    }

    /// Get the attributes of an element.
    pub(crate) fn attrs(&self, id: NodeId) -> Vec<Attribute> {
        let nodes = self.nodes.borrow();
        match &nodes[id].kind {
            NodeKind::Element { attrs, .. } => attrs.clone(),
            _ => Vec::new(),
        }
    }

    /// Concatenated class and id attribute values (lowercased), for scoring.
    pub(crate) fn class_and_id(&self, id: NodeId) -> String {
        let attrs = self.attrs(id);
        let mut out = String::new();
        for attr in &attrs {
            let name = attr.name.local.as_ref();
            if name == "class" || name == "id" {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&attr.value);
            }
        }
        out.to_ascii_lowercase()
    }

    /// Collect all text underneath a node, depth-first.
    pub(crate) fn inner_text(&self, id: NodeId) -> String {
        let mut buf = String::new();
        self.collect_text(id, &mut buf);
        buf
    }

    fn collect_text(&self, id: NodeId, buf: &mut String) {
        let nodes = self.nodes.borrow();
        match &nodes[id].kind {
            NodeKind::Text(t) => buf.push_str(t),
            _ => {
                // Vec<usize> copy — cheap, needed to release the borrow for recursion.
                let children = nodes[id].children.clone();
                drop(nodes);
                for child in children {
                    self.collect_text(child, buf);
                }
            }
        }
    }

    /// Remove a node from its parent's children list (detach).
    pub(crate) fn detach(&self, id: NodeId) {
        let parent_id = {
            let nodes = self.nodes.borrow();
            nodes[id].parent
        };
        if let Some(pid) = parent_id {
            let mut nodes = self.nodes.borrow_mut();
            nodes[pid].children.retain(|&c| c != id);
            nodes[id].parent = None;
        }
    }

    /// Allocate a new node and return its id.
    fn alloc(&self, kind: NodeKind) -> NodeId {
        let mut nodes = self.nodes.borrow_mut();
        let id = nodes.len();
        nodes.push(Node {
            kind,
            parent: None,
            children: Vec::new(),
        });
        id
    }

    /// Append `child` as the last child of `parent`.
    fn append_child(&self, parent: NodeId, child: NodeId) {
        let mut nodes = self.nodes.borrow_mut();
        nodes[child].parent = Some(parent);
        nodes[parent].children.push(child);
    }

    /// Append text to `parent`.  If the last child is already a text
    /// node, merge into it.
    fn append_text(&self, parent: NodeId, text: &str) {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(&last_id) = nodes[parent].children.last() {
            if let NodeKind::Text(ref mut existing) = nodes[last_id].kind {
                existing.push_str(text);
                return;
            }
        }
        // Allocate new text node.
        let id = nodes.len();
        nodes.push(Node {
            kind: NodeKind::Text(text.to_string()),
            parent: Some(parent),
            children: Vec::new(),
        });
        nodes[parent].children.push(id);
    }

    /// Insert `child` before `sibling` in sibling's parent.
    fn insert_before(&self, sibling: NodeId, child: NodeId) {
        let mut nodes = self.nodes.borrow_mut();
        let parent = match nodes[sibling].parent {
            Some(p) => p,
            None => global_infra::unrecoverable!("insert_before: sibling has no parent"),
        };
        nodes[child].parent = Some(parent);
        let pos = match nodes[parent].children.iter().position(|&c| c == sibling) {
            Some(p) => p,
            None => global_infra::unrecoverable!("insert_before: sibling not in parent's children"),
        };
        nodes[parent].children.insert(pos, child);
    }

    /// Insert text before `sibling`.
    fn insert_text_before(&self, sibling: NodeId, text: &str) {
        let mut nodes = self.nodes.borrow_mut();
        let parent = match nodes[sibling].parent {
            Some(p) => p,
            None => global_infra::unrecoverable!("insert_text_before: sibling has no parent"),
        };
        let pos = match nodes[parent].children.iter().position(|&c| c == sibling) {
            Some(p) => p,
            None => {
                global_infra::unrecoverable!("insert_text_before: sibling not in parent's children")
            }
        };
        // Check if the preceding sibling is text.
        if pos > 0 {
            let prev = nodes[parent].children[pos - 1];
            if let NodeKind::Text(ref mut existing) = nodes[prev].kind {
                existing.push_str(text);
                return;
            }
        }
        let id = nodes.len();
        nodes.push(Node {
            kind: NodeKind::Text(text.to_string()),
            parent: Some(parent),
            children: Vec::new(),
        });
        nodes[parent].children.insert(pos, id);
    }
}

// ---------------------------------------------------------------------------
// TreeSink implementation
// ---------------------------------------------------------------------------

/// Wrapper so we can return a reference to QualName for `ElemName`.
#[derive(Debug)]
pub(crate) struct ElemNameRef<'a> {
    name: &'a QualName,
}

impl html5ever::tree_builder::ElemName for ElemNameRef<'_> {
    fn ns(&self) -> &html5ever::Namespace {
        &self.name.ns
    }
    fn local_name(&self) -> &html5ever::LocalName {
        &self.name.local
    }
}

impl TreeSink for Dom {
    type Handle = NodeId;
    type Output = Self;
    type ElemName<'a> = ElemNameRef<'a>;

    fn finish(self) -> Self {
        self
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {
        // Silently ignore parse errors — malformed HTML is expected.
    }

    fn get_document(&self) -> NodeId {
        0
    }

    fn elem_name<'a>(&'a self, &target: &'a NodeId) -> ElemNameRef<'a> {
        // SAFETY: we hold &self so the RefCell borrow is valid for 'a
        // only if we don't mutably borrow nodes concurrently.  This
        // is safe because elem_name is only called during tree-building
        // which is single-threaded.
        let nodes = self.nodes.borrow();
        let ptr = match &nodes[target].kind {
            NodeKind::Element { name, .. } => name as *const QualName,
            _ => global_infra::unrecoverable!("elem_name called on non-element"),
        };
        ElemNameRef {
            // SAFETY: node storage is append-only (never reallocated
            // while the parser holds a reference) and the node is
            // never removed during parsing.
            name: unsafe { &*ptr },
        }
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> NodeId {
        let template_contents = if flags.template {
            Some(self.alloc(NodeKind::Document))
        } else {
            None
        };
        self.alloc(NodeKind::Element {
            name,
            attrs,
            template_contents,
        })
    }

    fn create_comment(&self, text: StrTendril) -> NodeId {
        self.alloc(NodeKind::Comment(text.to_string()))
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> NodeId {
        self.alloc(NodeKind::ProcessingInstruction {
            target: target.to_string(),
            data: data.to_string(),
        })
    }

    fn append(&self, &parent: &NodeId, child: NodeOrText<NodeId>) {
        match child {
            NodeOrText::AppendNode(id) => self.append_child(parent, id),
            NodeOrText::AppendText(t) => self.append_text(parent, &t),
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeId,
        prev_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        let has_parent = self.nodes.borrow()[*element].parent.is_some();
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let id = self.alloc(NodeKind::Doctype {
            name: name.to_string(),
            public_id: public_id.to_string(),
            system_id: system_id.to_string(),
        });
        self.append_child(0, id);
    }

    fn get_template_contents(&self, &target: &NodeId) -> NodeId {
        let nodes = self.nodes.borrow();
        match &nodes[target].kind {
            NodeKind::Element {
                template_contents: Some(id),
                ..
            } => *id,
            _ => global_infra::unrecoverable!("get_template_contents on non-template"),
        }
    }

    fn same_node(&self, &x: &NodeId, &y: &NodeId) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, &sibling: &NodeId, child: NodeOrText<NodeId>) {
        match child {
            NodeOrText::AppendNode(id) => self.insert_before(sibling, id),
            NodeOrText::AppendText(t) => self.insert_text_before(sibling, &t),
        }
    }

    fn add_attrs_if_missing(&self, &target: &NodeId, attrs: Vec<Attribute>) {
        let mut nodes = self.nodes.borrow_mut();
        if let NodeKind::Element {
            attrs: ref mut existing,
            ..
        } = nodes[target].kind
        {
            for attr in attrs {
                if !existing.iter().any(|a| a.name == attr.name) {
                    existing.push(attr);
                }
            }
        }
    }

    fn remove_from_parent(&self, &target: &NodeId) {
        self.detach(target);
    }

    fn reparent_children(&self, &node: &NodeId, &new_parent: &NodeId) {
        let children: Vec<NodeId> = {
            let nodes = self.nodes.borrow();
            nodes[node].children.clone()
        };
        let mut nodes = self.nodes.borrow_mut();
        nodes[node].children.clear();
        for child in children {
            nodes[child].parent = Some(new_parent);
            nodes[new_parent].children.push(child);
        }
    }
}

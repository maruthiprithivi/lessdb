//! LessGraph — an in-memory property graph for agent contexts.
//!
//! LessDB's answer to "drop our context into Claude Code / Codex / agentic
//! stacks instead of Obsidian or a standalone graph database": a fast,
//! RAM-resident property graph (nodes, typed directed edges, BFS
//! traversal, shortest paths, ranked search) with a context layer on top
//! (titled notes with tags and text, linked by typed edges), and JSON
//! snapshot persistence.
//!
//! Design points:
//! * adjacency lists (`out`/`in` edge ids) give O(degree) traversal;
//! * nodes are keyed by stable string ids — agents mint readable keys
//!   (`proj/lessdb`, `task/123`) instead of opaque numeric ids;
//! * search is ranked substring match over key/labels/properties — no
//!   index yet (context-scale data), an inverted index is on the roadmap;
//! * persistence is a single JSON snapshot, written atomically after every
//!   mutation (tmp + rename), so agents can crash and resume.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use less_common::{LessError, Result};

/// A graph node: stable key, labels, arbitrary JSON properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub key: String,
    pub labels: Vec<String>,
    pub props: Map<String, Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A typed edge between node keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub props: Map<String, Value>,
    pub directed: bool,
    pub created_at: i64,
}

/// Traversal direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
    Both,
}

impl Direction {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "out" | "outgoing" => Ok(Self::Out),
            "in" | "incoming" => Ok(Self::In),
            "both" => Ok(Self::Both),
            _ => Err(LessError::Config(format!(
                "unknown direction '{s}' (expected out|in|both)"
            ))),
        }
    }
}

/// A node reached by traversal.
#[derive(Debug, Clone, Serialize)]
pub struct Neighbor {
    pub node: String,
    /// Edge kind that led here (`None` for the starting node).
    pub via: Option<String>,
    pub depth: u32,
}

/// One hop of a path.
#[derive(Debug, Clone, Serialize)]
pub struct Hop {
    pub from: String,
    pub to: String,
    pub kind: String,
}

/// Serializable snapshot of a graph.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphSnapshot {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// In-memory property graph.
#[derive(Debug, Default, Clone)]
pub struct GraphStore {
    nodes: HashMap<String, Node>,
    edges: Vec<Edge>,
    out: HashMap<String, Vec<u32>>,
    inc: HashMap<String, Vec<u32>>,
}

impl GraphStore {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- nodes -----------------------------------------------------------

    /// Insert or update a node. Existing labels/props are merged (new keys
    /// overwrite old ones), timestamps bumped.
    pub fn upsert_node(
        &mut self,
        key: &str,
        labels: Vec<String>,
        props: Map<String, Value>,
    ) -> Node {
        let now = Utc::now().timestamp_millis();
        match self.nodes.get_mut(key) {
            Some(existing) => {
                for l in labels {
                    if !existing.labels.contains(&l) {
                        existing.labels.push(l);
                    }
                }
                existing.labels.sort();
                existing.props.extend(props);
                existing.updated_at = now;
                existing.clone()
            }
            None => {
                let node = Node {
                    key: key.to_string(),
                    labels,
                    props,
                    created_at: now,
                    updated_at: now,
                };
                self.nodes.insert(key.to_string(), node.clone());
                self.out.entry(key.to_string()).or_default();
                self.inc.entry(key.to_string()).or_default();
                node
            }
        }
    }

    pub fn get_node(&self, key: &str) -> Option<&Node> {
        self.nodes.get(key)
    }

    /// Delete a node and all edges touching it. Returns whether it existed.
    pub fn delete_node(&mut self, key: &str) -> bool {
        if self.nodes.remove(key).is_none() {
            return false;
        }
        self.out.remove(key);
        self.inc.remove(key);
        self.edges.retain(|e| e.from != key && e.to != key);
        self.rebuild_adjacency();
        true
    }

    fn rebuild_adjacency(&mut self) {
        self.out.clear();
        self.inc.clear();
        for key in self.nodes.keys() {
            self.out.insert(key.clone(), vec![]);
            self.inc.insert(key.clone(), vec![]);
        }
        for (id, e) in self.edges.iter().enumerate() {
            self.out.entry(e.from.clone()).or_default().push(id as u32);
            self.inc.entry(e.to.clone()).or_default().push(id as u32);
        }
    }

    /// Edge ids leaving a node.
    pub fn out_edges(&self, key: &str) -> Vec<u32> {
        self.out.get(key).cloned().unwrap_or_default()
    }

    /// Edge ids entering a node.
    pub fn in_edges(&self, key: &str) -> Vec<u32> {
        self.inc.get(key).cloned().unwrap_or_default()
    }

    /// Edge by internal id (ids are stable between mutations).
    pub fn edge_by_id(&self, id: u32) -> Option<Edge> {
        self.edges.get(id as usize).cloned()
    }

    /// All nodes (unordered).
    pub fn nodes(&self) -> Vec<Node> {
        self.nodes.values().cloned().collect()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn node_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.nodes.keys().cloned().collect();
        keys.sort();
        keys
    }

    // ---- edges -----------------------------------------------------------

    /// Insert or update an edge. Missing endpoints are auto-created as bare
    /// nodes (agent-friendly), and re-upserting the same (from, to, kind)
    /// replaces its properties.
    pub fn upsert_edge(
        &mut self,
        from: &str,
        to: &str,
        kind: &str,
        props: Map<String, Value>,
        directed: bool,
    ) -> Result<Edge> {
        if from == to {
            return Err(LessError::Config(
                "self-loop edges are not supported".into(),
            ));
        }
        // Auto-create endpoints.
        if !self.nodes.contains_key(from) {
            self.upsert_node(from, vec![], Map::new());
        }
        if !self.nodes.contains_key(to) {
            self.upsert_node(to, vec![], Map::new());
        }
        let now = Utc::now().timestamp_millis();
        if let Some(existing) = self
            .edges
            .iter_mut()
            .find(|e| e.from == from && e.to == to && e.kind == kind)
        {
            existing.props = props;
            existing.directed = directed;
            return Ok(existing.clone());
        }
        let edge = Edge {
            from: from.to_string(),
            to: to.to_string(),
            kind: kind.to_string(),
            props,
            directed,
            created_at: now,
        };
        self.edges.push(edge.clone());
        self.rebuild_adjacency();
        Ok(edge)
    }

    /// Remove all edges matching `(from, to)` (and `kind` when given).
    pub fn delete_edges(&mut self, from: &str, to: &str, kind: Option<&str>) -> usize {
        let before = self.edges.len();
        self.edges
            .retain(|e| !(e.from == from && e.to == to && kind.is_none_or(|k| e.kind == k)));
        let removed = before - self.edges.len();
        if removed > 0 {
            self.rebuild_adjacency();
        }
        removed
    }

    // ---- traversal -------------------------------------------------------

    /// Breadth-first neighborhood within `max_depth` (0 = unlimited).
    pub fn neighbors(
        &self,
        key: &str,
        direction: Direction,
        max_depth: u32,
    ) -> Result<Vec<Neighbor>> {
        if !self.nodes.contains_key(key) {
            return Err(LessError::Config(format!("unknown node '{key}'")));
        }
        let mut out = vec![];
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(key.to_string());
        let mut queue: VecDeque<(String, u32)> = VecDeque::new();
        queue.push_back((key.to_string(), 0));
        while let Some((current, depth)) = queue.pop_front() {
            if max_depth > 0 && depth >= max_depth {
                continue;
            }
            let mut next: Vec<(String, u32, String)> = vec![];
            if direction != Direction::In {
                for id in self.out.get(&current).into_iter().flatten() {
                    if let Some(e) = self.edges.get(*id as usize)
                        && !visited.contains(&e.to)
                    {
                        next.push((e.to.clone(), depth + 1, e.kind.clone()));
                    }
                }
            }
            if direction != Direction::Out {
                for id in self.inc.get(&current).into_iter().flatten() {
                    if let Some(e) = self.edges.get(*id as usize)
                        && !visited.contains(&e.from)
                    {
                        next.push((e.from.clone(), depth + 1, e.kind.clone()));
                    }
                }
            }
            for (n, d, via) in next {
                if visited.insert(n.clone()) {
                    out.push(Neighbor {
                        node: n.clone(),
                        via: Some(via),
                        depth: d,
                    });
                    queue.push_back((n, d));
                }
            }
        }
        Ok(out)
    }

    /// BFS shortest path (unweighted) between two nodes.
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<Hop>> {
        if !self.nodes.contains_key(from) || !self.nodes.contains_key(to) {
            return None;
        }
        if from == to {
            return Some(vec![]);
        }
        let mut prev: HashMap<String, (String, String)> = HashMap::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        visited.insert(from.to_string());
        queue.push_back(from.to_string());
        while let Some(current) = queue.pop_front() {
            let mut expansions: Vec<(String, String, String)> = vec![];
            for id in self.out.get(&current).into_iter().flatten() {
                if let Some(e) = self.edges.get(*id as usize) {
                    expansions.push((current.clone(), e.to.clone(), e.kind.clone()));
                }
            }
            for id in self.inc.get(&current).into_iter().flatten() {
                if let Some(e) = self.edges.get(*id as usize) {
                    expansions.push((current.clone(), e.from.clone(), e.kind.clone()));
                }
            }
            for (src, dst, kind) in expansions {
                if visited.insert(dst.clone()) {
                    prev.insert(dst.clone(), (src, kind));
                    if dst == to {
                        // Reconstruct.
                        let mut hops = vec![];
                        let mut cursor = to.to_string();
                        while cursor != from {
                            let (p, k) = prev.get(&cursor)?.clone();
                            hops.push(Hop {
                                from: p.clone(),
                                to: cursor.clone(),
                                kind: k,
                            });
                            cursor = p;
                        }
                        hops.reverse();
                        return Some(hops);
                    }
                    queue.push_back(dst);
                }
            }
        }
        None
    }

    // ---- search ----------------------------------------------------------

    /// Ranked substring search over keys, labels and string properties.
    /// Returns `(key, score)`; score: 4 = key prefix match, 3 = key
    /// substring, 2 = label match, 1 = property value match.
    pub fn search_ranked(&self, query: &str, limit: usize) -> Vec<(String, i32)> {
        let q = query.to_ascii_lowercase();
        let mut hits: Vec<(String, i32)> = vec![];
        for node in self.nodes.values() {
            let key = node.key.to_ascii_lowercase();
            let mut score = 0i32;
            if key.starts_with(&q) {
                score = score.max(4);
            } else if key.contains(&q) {
                score = score.max(3);
            }
            if node
                .labels
                .iter()
                .any(|l| l.to_ascii_lowercase().contains(&q))
            {
                score = score.max(2);
            }
            let prop_hit = node.props.values().any(|v| match v {
                Value::String(s) => s.to_ascii_lowercase().contains(&q),
                _ => false,
            });
            if prop_hit {
                score = score.max(1);
            }
            if score > 0 {
                hits.push((node.key.clone(), score));
            }
        }
        hits.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        hits.truncate(limit);
        hits
    }

    // ---- persistence -----------------------------------------------------

    pub fn snapshot(&self) -> GraphSnapshot {
        let mut nodes: Vec<Node> = self.nodes.values().cloned().collect();
        nodes.sort_by(|a, b| a.key.cmp(&b.key));
        GraphSnapshot {
            nodes,
            edges: self.edges.clone(),
        }
    }

    pub fn load(&mut self, snapshot: GraphSnapshot) {
        for node in snapshot.nodes {
            self.nodes.insert(node.key.clone(), node);
        }
        self.edges = snapshot.edges;
        self.rebuild_adjacency();
    }
}

// ===========================================================================
// Context layer: titled, tagged, linked notes for agents.
// ===========================================================================

/// A context entity (a titled note with tags and body text).
#[derive(Debug, Clone, Serialize)]
pub struct Context {
    pub key: String,
    pub title: String,
    pub text: String,
    pub tags: Vec<String>,
    pub kind: String,
    pub props: Map<String, Value>,
    pub updated_at: i64,
}

/// A search hit summary (snippet of the body text).
#[derive(Debug, Clone, Serialize)]
pub struct ContextHit {
    pub key: String,
    pub title: String,
    pub tags: Vec<String>,
    pub kind: String,
    pub snippet: String,
    pub score: i32,
}

/// Aggregate context statistics.
#[derive(Debug, Clone, Serialize)]
pub struct ContextStats {
    pub nodes: usize,
    pub edges: usize,
}

const CONTEXT_LABEL: &str = "context";
const TITLE: &str = "title";
const TEXT: &str = "text";
const TAGS: &str = "tags";
const KIND: &str = "kind";

/// A [`GraphStore`] plus optional JSON persistence, with context semantics.
///
/// Mutations mark the store dirty instead of writing on every call (the
/// snapshot write is O(n); write-per-mutation made bulk loads O(n²) — the
/// benchmark suite caught this). [`ContextStore::flush`] persists when
/// dirty, and `Drop` persists best-effort, so CLI one-shots stay durable.
pub struct ContextStore {
    graph: GraphStore,
    path: Option<PathBuf>,
    dirty: bool,
}

impl ContextStore {
    /// Open a context store, loading `<dir>/graph.json` if present.
    /// `dir` is `<data_dir>/memory` (created on first flush).
    pub fn open(dir: Option<&Path>) -> Result<Self> {
        let graph = match dir {
            Some(dir) => {
                let file = dir.join("graph.json");
                if file.exists() {
                    let bytes = std::fs::read(&file)?;
                    let snapshot: GraphSnapshot = serde_json::from_slice(&bytes)?;
                    let mut g = GraphStore::new();
                    g.load(snapshot);
                    g
                } else {
                    GraphStore::new()
                }
            }
            None => GraphStore::new(),
        };
        Ok(Self {
            graph,
            path: dir.map(|p| p.to_path_buf()),
            dirty: false,
        })
    }

    pub fn graph(&self) -> &GraphStore {
        &self.graph
    }

    /// Mutable access for write statements (Cypher CREATE/DELETE/SET).
    pub fn graph_mut(&mut self) -> &mut GraphStore {
        self.dirty = true;
        &mut self.graph
    }

    fn persist(&self) -> Result<()> {
        if let Some(dir) = &self.path {
            std::fs::create_dir_all(dir)?;
            let tmp = dir.join("graph.json.tmp");
            std::fs::write(&tmp, serde_json::to_string_pretty(&self.graph.snapshot())?)?;
            std::fs::rename(&tmp, dir.join("graph.json"))?;
        }
        Ok(())
    }

    /// Persist the snapshot when there are pending mutations. Mutating
    /// methods mark the store dirty; long-lived processes (the MCP server)
    /// call this after writes. `Drop` also flushes best-effort.
    pub fn flush(&mut self) -> Result<()> {
        if self.dirty {
            self.persist()?;
            self.dirty = false;
        }
        Ok(())
    }

    /// Put (create or update) a context. Props are merged on update.
    pub fn put(
        &mut self,
        key: &str,
        title: &str,
        text: &str,
        tags: Vec<String>,
        kind: &str,
        extra: Map<String, Value>,
    ) -> Result<Context> {
        let mut props = extra;
        props.insert(TITLE.into(), Value::String(title.to_string()));
        props.insert(TEXT.into(), Value::String(text.to_string()));
        props.insert(TAGS.into(), serde_json::to_value(tags)?);
        props.insert(KIND.into(), Value::String(kind.to_string()));
        let labels = vec![CONTEXT_LABEL.to_string(), kind.to_string()];
        let node = self.graph.upsert_node(key, labels, props);
        self.dirty = true;
        context_from_node(&node).ok_or_else(|| {
            LessError::Engine(format!("stored context '{key}' has malformed properties"))
        })
    }

    pub fn get(&self, key: &str) -> Option<Context> {
        self.graph.get_node(key).and_then(context_from_node)
    }

    /// Ranked search over contexts, with a body snippet around the match.
    pub fn find(&self, query: &str, limit: usize) -> Vec<ContextHit> {
        self.graph
            .search_ranked(query, limit.max(1))
            .into_iter()
            .filter_map(|(key, score)| {
                let ctx = context_from_node(self.graph.get_node(&key)?)?;
                let snippet = snippet(&ctx.text, query, 160);
                Some(ContextHit {
                    key,
                    title: ctx.title,
                    tags: ctx.tags,
                    kind: ctx.kind,
                    snippet,
                    score,
                })
            })
            .collect()
    }

    /// Link two contexts (or any node) with a typed edge.
    pub fn link(
        &mut self,
        from: &str,
        to: &str,
        kind: &str,
        directed: bool,
        props: Map<String, Value>,
    ) -> Result<Edge> {
        let edge = self.graph.upsert_edge(from, to, kind, props, directed)?;
        self.dirty = true;
        Ok(edge)
    }

    /// Unlink; returns the number of edges removed.
    pub fn unlink(&mut self, from: &str, to: &str, kind: Option<&str>) -> Result<usize> {
        let n = self.graph.delete_edges(from, to, kind);
        if n > 0 {
            self.dirty = true;
        }
        Ok(n)
    }

    pub fn delete(&mut self, key: &str) -> Result<bool> {
        let removed = self.graph.delete_node(key);
        if removed {
            self.dirty = true;
        }
        Ok(removed)
    }

    pub fn neighbors(
        &self,
        key: &str,
        direction: Direction,
        max_depth: u32,
    ) -> Result<Vec<Neighbor>> {
        self.graph.neighbors(key, direction, max_depth)
    }

    pub fn path(&self, from: &str, to: &str) -> Option<Vec<Hop>> {
        self.graph.shortest_path(from, to)
    }

    pub fn stats(&self) -> ContextStats {
        ContextStats {
            nodes: self.graph.node_count(),
            edges: self.graph.edge_count(),
        }
    }
}

impl Drop for ContextStore {
    fn drop(&mut self) {
        if self.dirty {
            let _ = self.persist();
        }
    }
}

fn context_from_node(node: &Node) -> Option<Context> {
    let title = node.props.get(TITLE)?.as_str()?.to_string();
    let text = node
        .props
        .get(TEXT)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tags = node
        .props
        .get(TAGS)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let kind = node
        .props
        .get(KIND)
        .and_then(|v| v.as_str())
        .unwrap_or("note")
        .to_string();
    Some(Context {
        key: node.key.clone(),
        title,
        text,
        tags,
        kind,
        props: node.props.clone(),
        updated_at: node.updated_at,
    })
}

/// A short excerpt of `text` centered on the first occurrence of `query`.
fn snippet(text: &str, query: &str, radius: usize) -> String {
    let lower = text.to_ascii_lowercase();
    let q = query.to_ascii_lowercase();
    let idx = lower.find(&q).unwrap_or(0);
    let start = idx.saturating_sub(radius / 2);
    let mut snippet = text.chars().skip(start).take(radius).collect::<String>();
    if start > 0 {
        snippet.insert(0, '…');
    }
    if idx + radius < text.chars().count() {
        snippet.push('…');
    }
    snippet.replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> ContextStore {
        ContextStore::open(None).unwrap()
    }

    #[test]
    fn context_put_get_find() {
        let mut s = store();
        s.put(
            "proj/lessdb",
            "LessDB",
            "A fast analytical database in Rust.",
            vec!["db".into(), "rust".into()],
            "project",
            Map::new(),
        )
        .unwrap();
        s.put(
            "note/ideas",
            "Ideas",
            "GPU kernels for aggregation and joins.",
            vec!["gpu".into()],
            "note",
            Map::new(),
        )
        .unwrap();

        let ctx = s.get("proj/lessdb").unwrap();
        assert_eq!(ctx.title, "LessDB");
        assert_eq!(ctx.tags, vec!["db", "rust"]);

        let hits = s.find("gpu", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "note/ideas");
        assert!(hits[0].snippet.contains("GPU"));

        // Update merges props and keeps tags semantics (replace tags).
        s.put(
            "proj/lessdb",
            "LessDB",
            "updated text",
            vec!["analytics".into()],
            "project",
            Map::new(),
        )
        .unwrap();
        let ctx = s.get("proj/lessdb").unwrap();
        assert_eq!(ctx.text, "updated text");
        assert_eq!(ctx.tags, vec!["analytics"]);
        assert_eq!(s.stats().nodes, 2);
    }

    #[test]
    fn links_and_traversal() {
        let mut s = store();
        for (k, t) in [("a", "A"), ("b", "B"), ("c", "C"), ("d", "D")] {
            s.put(k, t, "text", vec![], "note", Map::new()).unwrap();
        }
        s.link("a", "b", "depends_on", true, Map::new()).unwrap();
        s.link("b", "c", "depends_on", true, Map::new()).unwrap();
        s.link("a", "d", "mentions", false, Map::new()).unwrap();

        let n = s.neighbors("a", Direction::Out, 1).unwrap();
        let mut keys: Vec<&str> = n.iter().map(|x| x.node.as_str()).collect();
        keys.sort();
        assert_eq!(keys, vec!["b", "d"]);

        // Depth 2 reaches c through b.
        let n = s.neighbors("a", Direction::Out, 2).unwrap();
        assert!(n.iter().any(|x| x.node == "c" && x.depth == 2));

        let path = s.path("a", "c").unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].kind, "depends_on");
        assert_eq!(path[1].to, "c");

        // In-direction traversal from c reaches a.
        let n = s.neighbors("c", Direction::In, 1).unwrap();
        assert!(n.iter().any(|x| x.node == "b"));

        s.unlink("a", "b", None).unwrap();
        assert!(s.path("a", "c").is_none());
    }

    #[test]
    fn delete_node_cascades_edges() {
        let mut s = store();
        s.put("a", "A", "t", vec![], "note", Map::new()).unwrap();
        s.put("b", "B", "t", vec![], "note", Map::new()).unwrap();
        s.link("a", "b", "x", true, Map::new()).unwrap();
        assert_eq!(s.stats().edges, 1);
        s.delete("a").unwrap();
        assert_eq!(s.stats().nodes, 1);
        assert_eq!(s.stats().edges, 0);
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!("less-ctx-{}", uuid::Uuid::new_v4()));
        {
            let mut s = ContextStore::open(Some(&dir)).unwrap();
            s.put("k", "Title", "body", vec!["t".into()], "note", Map::new())
                .unwrap();
            s.link("k", "other", "ref", true, Map::new()).unwrap();
        }
        let s = ContextStore::open(Some(&dir)).unwrap();
        assert_eq!(s.stats().nodes, 2);
        assert_eq!(s.stats().edges, 1);
        assert_eq!(s.get("k").unwrap().title, "Title");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

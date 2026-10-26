//! A copy-on-write entity store with draw order.
//!
//! Entities live in chunks (`Arc<Vec<Arc<Entity>>>`) in draw order, and a sharded handle index
//! maps each handle to the stable id of its chunk. Cloning a store (an undo snapshot) copies
//! only the chunk and shard pointers; an edit copies one chunk and one shard.
//!
//! A lookup by handle is two hash lookups (the handle's chunk id, the chunk's position) and a
//! scan of the chunk's handles, kept beside its entities so the scan reads one contiguous array
//! rather than chasing a pointer per entity. Looking up every object of a large drawing (a
//! selection of all of them) used to spend most of its time finding the chunk by scanning the
//! chunk list.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{Entity, Handle};

const CHUNK_MAX: usize = 1024;
const SHARDS: usize = 64;

#[derive(Clone, Debug, Default)]
struct Chunk {
    id: u32,
    items: Vec<Arc<Entity>>,
    /// `items[i].handle`, for the lookup scan.
    handles: Vec<Handle>,
}

impl Chunk {
    fn find(&self, h: Handle) -> Option<usize> {
        self.handles.iter().position(|&x| x == h)
    }
    fn push(&mut self, e: Arc<Entity>) {
        self.handles.push(e.handle);
        self.items.push(e);
    }
    fn insert(&mut self, i: usize, e: Arc<Entity>) {
        self.handles.insert(i, e.handle);
        self.items.insert(i, e);
    }
    fn remove(&mut self, i: usize) -> Arc<Entity> {
        self.handles.remove(i);
        self.items.remove(i)
    }
    fn split_off(&mut self, at: usize) -> Chunk {
        Chunk { id: 0, items: self.items.split_off(at), handles: self.handles.split_off(at) }
    }
}

#[derive(Clone, Debug)]
pub struct EntityStore {
    chunks: Vec<Arc<Chunk>>,
    /// Chunk id → its position in `chunks`; remade whenever a chunk comes, goes or moves.
    positions: Arc<HashMap<u32, usize>>,
    shards: Vec<Arc<HashMap<Handle, u32>>>,
    len: usize,
    next_chunk: u32,
}

impl Default for EntityStore {
    fn default() -> Self {
        EntityStore {
            chunks: Vec::new(),
            positions: Arc::default(),
            shards: (0..SHARDS).map(|_| Arc::new(HashMap::new())).collect(),
            len: 0,
            next_chunk: 0,
        }
    }
}

impl PartialEq for EntityStore {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

fn shard_of(h: Handle) -> usize {
    // Fibonacci hashing spreads sequential handles across shards.
    ((h.0.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 58) as usize) % SHARDS
}

impl EntityStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Iterate in draw order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<Entity>> + '_ {
        self.chunks.iter().flat_map(|c| c.items.iter())
    }
    pub fn handles(&self) -> Vec<Handle> {
        self.iter().map(|e| e.handle).collect()
    }
    pub fn contains(&self, h: Handle) -> bool {
        self.chunk_id(h).is_some()
    }
    fn chunk_id(&self, h: Handle) -> Option<u32> {
        self.shards.get(shard_of(h)).and_then(|s| s.get(&h).copied())
    }
    fn chunk_pos(&self, id: u32) -> Option<usize> {
        self.positions.get(&id).copied()
    }
    /// Remake `positions` after `chunks` changed.
    fn place_chunks(&mut self) {
        self.positions = Arc::new(self.chunks.iter().enumerate().map(|(i, c)| (c.id, i)).collect());
    }
    pub fn get(&self, h: Handle) -> Option<&Arc<Entity>> {
        let chunk = self.chunks.get(self.chunk_pos(self.chunk_id(h)?)?)?;
        chunk.items.get(chunk.find(h)?)
    }
    fn set_index(&mut self, h: Handle, chunk: Option<u32>) {
        if let Some(shard) = self.shards.get_mut(shard_of(h)) {
            let m = Arc::make_mut(shard);
            match chunk {
                Some(c) => {
                    m.insert(h, c);
                }
                None => {
                    m.remove(&h);
                }
            }
        }
    }
    fn new_chunk(&mut self) -> Arc<Chunk> {
        let id = self.next_chunk;
        self.next_chunk = self.next_chunk.wrapping_add(1);
        Arc::new(Chunk { id, items: Vec::with_capacity(64), handles: Vec::with_capacity(64) })
    }
    /// Append on top of the draw order. Replaces an entity with the same handle in place.
    pub fn push(&mut self, e: Entity) {
        if self.contains(e.handle) {
            self.replace(e);
            return;
        }
        self.push_new(Arc::new(e));
    }
    /// Append `e`, which is not in the store, on top of the draw order.
    fn push_new(&mut self, e: Arc<Entity>) {
        let needs_new = self.chunks.last().is_none_or(|c| c.items.len() >= CHUNK_MAX);
        if needs_new {
            let c = self.new_chunk();
            self.chunks.push(c);
            self.place_chunks();
        }
        let h = e.handle;
        let Some(last) = self.chunks.last_mut() else { return };
        let chunk = Arc::make_mut(last);
        chunk.push(e);
        let id = chunk.id;
        self.set_index(h, Some(id));
        self.len += 1;
    }
    /// Insert at the bottom of the draw order.
    pub fn push_front(&mut self, e: Entity) {
        if self.contains(e.handle) {
            self.replace(e);
            return;
        }
        let needs_new = self.chunks.first().is_none_or(|c| c.items.len() >= CHUNK_MAX);
        if needs_new {
            let c = self.new_chunk();
            self.chunks.insert(0, c);
            self.place_chunks();
        }
        let h = e.handle;
        let Some(first) = self.chunks.first_mut() else { return };
        let chunk = Arc::make_mut(first);
        chunk.insert(0, Arc::new(e));
        let id = chunk.id;
        self.set_index(h, Some(id));
        self.len += 1;
    }
    /// Replace the entity with the same handle, keeping its draw position. Returns false if absent.
    pub fn replace(&mut self, e: Entity) -> bool {
        let Some(id) = self.chunk_id(e.handle) else { return false };
        let Some(pos) = self.chunk_pos(id) else { return false };
        let Some(c) = self.chunks.get_mut(pos) else { return false };
        let chunk = Arc::make_mut(c);
        match chunk.find(e.handle).and_then(|i| chunk.items.get_mut(i)) {
            Some(slot) => {
                *slot = Arc::new(e);
                true
            }
            None => false,
        }
    }
    pub fn remove(&mut self, h: Handle) -> Option<Arc<Entity>> {
        let id = self.chunk_id(h)?;
        let pos = self.chunk_pos(id)?;
        let c = self.chunks.get_mut(pos)?;
        let chunk = Arc::make_mut(c);
        let i = chunk.find(h)?;
        let e = chunk.remove(i);
        if chunk.items.is_empty() {
            self.chunks.remove(pos);
            self.place_chunks();
        }
        self.set_index(h, None);
        self.len = self.len.saturating_sub(1);
        Some(e)
    }
    /// Modify an entity in place through a closure (copy-on-write).
    pub fn modify<F: FnOnce(&mut Entity)>(&mut self, h: Handle, f: F) -> bool {
        let Some(id) = self.chunk_id(h) else { return false };
        let Some(pos) = self.chunk_pos(id) else { return false };
        let Some(c) = self.chunks.get_mut(pos) else { return false };
        let chunk = Arc::make_mut(c);
        match chunk.find(h).and_then(|i| chunk.items.get_mut(i)) {
            Some(slot) => {
                f(Arc::make_mut(slot));
                true
            }
            None => false,
        }
    }
    pub fn bring_to_front(&mut self, h: Handle) -> bool {
        match self.remove(h) {
            Some(e) => {
                self.push_new(e);
                true
            }
            None => false,
        }
    }
    pub fn send_to_back(&mut self, h: Handle) -> bool {
        match self.remove(h) {
            Some(e) => {
                self.push_front(Arc::unwrap_or_clone(e));
                true
            }
            None => false,
        }
    }
    /// Move `h` directly above (`above`) or under `anchor` in the draw order. False (and nothing
    /// moves) when either is absent or they are the same object.
    pub fn move_next_to(&mut self, h: Handle, anchor: Handle, above: bool) -> bool {
        if h == anchor || !self.contains(anchor) {
            return false;
        }
        let Some(e) = self.remove(h) else { return false };
        let at = self.chunk_id(anchor).and_then(|id| Some((id, self.chunk_pos(id)?)));
        let Some((id, pos)) = at else {
            // Unreachable in practice (the anchor was present); keep the object on top.
            self.push_new(e);
            return false;
        };
        let Some(c) = self.chunks.get_mut(pos) else {
            // Unreachable in practice too; never lose the object.
            self.push_new(e);
            return false;
        };
        let chunk = Arc::make_mut(c);
        let i = chunk.find(anchor).unwrap_or(chunk.items.len());
        let i = if above { (i + 1).min(chunk.items.len()) } else { i };
        chunk.insert(i, e);
        let split = chunk.items.len() > 2 * CHUNK_MAX;
        self.set_index(h, Some(id));
        self.len += 1;
        if split {
            self.split_chunk(pos);
        }
        true
    }
    /// Split an oversized chunk in two, keeping the order.
    fn split_chunk(&mut self, pos: usize) {
        let mut fresh = self.new_chunk();
        let Some(c) = self.chunks.get_mut(pos) else { return };
        let chunk = Arc::make_mut(c);
        let tail = chunk.split_off(chunk.items.len() / 2);
        let new = Arc::make_mut(&mut fresh);
        new.items = tail.items;
        new.handles = tail.handles;
        let (id, moved) = (new.id, new.handles.clone());
        self.chunks.insert(pos + 1, fresh);
        self.place_chunks();
        for h in moved {
            self.set_index(h, Some(id));
        }
    }
    /// Reorder the whole store by a key (stable: equal keys keep their order).
    pub fn sort_by_key<K: Ord>(&mut self, mut key: impl FnMut(&Entity) -> K) {
        let mut items: Vec<Arc<Entity>> = self.iter().cloned().collect();
        items.sort_by_cached_key(|e| key(e));
        *self = EntityStore::default();
        for e in items {
            self.push_new(e);
        }
    }
    /// True when `other` shares every chunk with `self` (same draw order, same entities). Holding
    /// a clone of a store makes this exact: any edit to the original copies the touched chunk.
    pub fn same_as(&self, other: &EntityStore) -> bool {
        self.len == other.len && self.chunks.len() == other.chunks.len() && self.chunks.iter().zip(&other.chunks).all(|(a, b)| Arc::ptr_eq(a, b))
    }
    /// The chunks in draw order as (identity key, entities). A key is stable for as long as any
    /// clone of this store is alive and changes whenever the chunk is edited (copy-on-write), so
    /// caches holding a clone can reuse per-chunk work.
    pub fn chunk_slices(&self) -> impl Iterator<Item = (usize, &[Arc<Entity>])> + '_ {
        self.chunks.iter().map(|c| (Arc::as_ptr(c) as usize, c.items.as_slice()))
    }
    /// The last entity in draw order (AutoCAD's "Last" selection).
    pub fn last(&self) -> Option<&Arc<Entity>> {
        self.chunks.last().and_then(|c| c.items.last())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityKind, Point};
    use cadcraft_geom::Vec2;

    fn ent(h: u64) -> Entity {
        Entity::new(Handle(h), EntityKind::Point(Point { p: Vec2::new(h as f64, 0.0).to3(0.0), angle: 0.0 }))
    }

    #[test]
    fn push_get_remove() {
        let mut s = EntityStore::new();
        for h in 1..=3000 {
            s.push(ent(h));
        }
        assert_eq!(s.len(), 3000);
        assert!(s.get(Handle(1500)).is_some());
        assert!(s.remove(Handle(1500)).is_some());
        assert!(s.get(Handle(1500)).is_none());
        assert_eq!(s.len(), 2999);
        assert_eq!(s.iter().count(), 2999);
    }

    #[test]
    fn snapshots_are_independent() {
        let mut s = EntityStore::new();
        for h in 1..=10 {
            s.push(ent(h));
        }
        let snap = s.clone();
        s.remove(Handle(3));
        s.modify(Handle(4), |e| e.common.layer = "X".into());
        assert!(snap.get(Handle(3)).is_some());
        assert_eq!(snap.get(Handle(4)).map(|e| e.common.layer.as_str()), Some("0"));
        assert_eq!(s.get(Handle(4)).map(|e| e.common.layer.as_str()), Some("X"));
    }

    #[test]
    fn draw_order_moves() {
        let mut s = EntityStore::new();
        for h in 1..=5 {
            s.push(ent(h));
        }
        s.bring_to_front(Handle(1));
        assert_eq!(s.last().map(|e| e.handle), Some(Handle(1)));
        s.send_to_back(Handle(5));
        assert_eq!(s.iter().next().map(|e| e.handle), Some(Handle(5)));
        assert_eq!(s.len(), 5);
    }

    fn order(s: &EntityStore) -> Vec<u64> {
        s.iter().map(|e| e.handle.0).collect()
    }

    #[test]
    fn move_next_to_and_sort() {
        let mut s = EntityStore::new();
        for h in 1..=5 {
            s.push(ent(h));
        }
        assert!(s.move_next_to(Handle(1), Handle(3), true));
        assert_eq!(order(&s), [2, 3, 1, 4, 5]);
        assert!(s.move_next_to(Handle(5), Handle(2), false));
        assert_eq!(order(&s), [5, 2, 3, 1, 4]);
        assert!(!s.move_next_to(Handle(5), Handle(5), true));
        assert!(!s.move_next_to(Handle(5), Handle(99), true));
        assert!(!s.move_next_to(Handle(99), Handle(5), true));
        assert_eq!(order(&s), [5, 2, 3, 1, 4]);
        assert_eq!(s.len(), 5);
        s.sort_by_key(|e| e.handle.0 % 2);
        assert_eq!(order(&s), [2, 4, 5, 3, 1]);
        assert!(s.get(Handle(3)).is_some());
    }

    #[test]
    fn move_next_to_splits_big_chunks() {
        let mut s = EntityStore::new();
        for h in 1..=3000 {
            s.push(ent(h));
        }
        // Pile everything into the first chunk, under handle 1.
        for h in 1500..=3000 {
            assert!(s.move_next_to(Handle(h), Handle(1), true));
        }
        assert_eq!(s.len(), 3000);
        assert!(s.chunks.iter().all(|c| c.items.len() <= 2 * CHUNK_MAX));
        let o = order(&s);
        assert_eq!(o.first(), Some(&1));
        assert_eq!(o.get(1), Some(&3000));
        for h in 1..=3000 {
            assert_eq!(s.get(Handle(h)).map(|e| e.handle.0), Some(h));
        }
    }

    /// Every handle in the shard index names a chunk that is placed, which lists it where its
    /// entity is.
    fn check(s: &EntityStore) {
        assert_eq!(s.positions.len(), s.chunks.len());
        for (i, c) in s.chunks.iter().enumerate() {
            assert_eq!(s.positions.get(&c.id), Some(&i));
            assert_eq!(c.handles, c.items.iter().map(|e| e.handle).collect::<Vec<_>>());
            assert!(!c.items.is_empty());
        }
        let indexed: usize = s.shards.iter().map(|m| m.len()).sum();
        assert_eq!((indexed, s.iter().count()), (s.len(), s.len()));
        for e in s.iter() {
            assert!(Arc::ptr_eq(s.get(e.handle).unwrap(), e));
        }
    }

    /// Lookups stay right through every kind of edit, in a long random mix of them, with
    /// snapshots taken along the way left as they were. The edit and its variant are drawn
    /// apart, so each path runs (and the test says so).
    #[test]
    fn lookups_survive_every_edit() {
        let mut s = EntityStore::new();
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut live: Vec<u64> = Vec::new();
        let mut snapshots: Vec<(EntityStore, Vec<u64>)> = Vec::new();
        let mut ran: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for step in 0..6000u64 {
            let (r, variant) = (next(), next() % 2 == 0);
            let pick = |live: &[u64], r: u64| live.get((r >> 8) as usize % live.len().max(1)).copied();
            match r % 9 {
                0..=3 => {
                    let h = step + 1;
                    if variant {
                        s.push(ent(h));
                        ran.insert("push");
                    } else {
                        s.push_front(ent(h));
                        ran.insert("push_front");
                    }
                    live.push(h);
                }
                4 => {
                    if let Some(h) = pick(&live, r) {
                        assert!(s.remove(Handle(h)).is_some());
                        live.retain(|&x| x != h);
                        ran.insert("remove");
                    }
                }
                5 => {
                    if let Some(h) = pick(&live, r) {
                        assert!(s.modify(Handle(h), |e| e.common.layer = "M".into()));
                        assert_eq!(s.get(Handle(h)).map(|e| e.common.layer.as_str()), Some("M"));
                        ran.insert("modify");
                    }
                }
                6 => {
                    if let Some(h) = pick(&live, r) {
                        if variant {
                            assert!(s.bring_to_front(Handle(h)));
                            ran.insert("bring_to_front");
                        } else {
                            assert!(s.send_to_back(Handle(h)));
                            ran.insert("send_to_back");
                        }
                    }
                }
                7 => {
                    if let (Some(h), Some(a)) = (pick(&live, r), pick(&live, next())) {
                        assert_eq!(s.move_next_to(Handle(h), Handle(a), variant), h != a);
                        ran.insert(if variant { "move above" } else { "move under" });
                    }
                }
                _ => {
                    if let Some(h) = pick(&live, r) {
                        assert!(s.replace(ent(h)));
                        ran.insert("replace");
                    }
                }
            }
            if step % 500 == 0 {
                snapshots.push((s.clone(), live.clone()));
            }
            if step % 97 == 0 {
                check(&s);
            }
        }
        assert_eq!(ran.len(), 9, "every edit path ran: {ran:?}");
        assert_eq!(snapshots.len(), 12);
        check(&s);
        assert_eq!(s.len(), live.len());
        for h in &live {
            assert_eq!(s.get(Handle(*h)).map(|e| e.handle.0), Some(*h));
        }
        s.sort_by_key(|e| std::cmp::Reverse(e.handle.0));
        check(&s);
        for (snap, had) in &snapshots {
            check(snap);
            assert_eq!(snap.len(), had.len());
            for h in had {
                assert!(snap.get(Handle(*h)).is_some());
            }
        }
    }
}

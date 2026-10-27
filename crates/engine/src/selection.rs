//! The selected objects of a drawing, with a generation that stands for their contents.

use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};

use cadcraft_doc::Handle;

/// The selected objects of a drawing.
///
/// Readers borrow the handles as a slice. Every change goes through [`Selection::set`] or
/// [`Selection::clear`], which give the selection a new generation: equal generations hold equal
/// handles (the empty selection is generation 0), so work kept per selection and looked at every
/// frame (the Properties palette's summary, the dynamic grips, the command line's typed-since
/// flag) is reused after comparing two numbers rather than every handle.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    handles: Vec<Handle>,
    generation: u64,
}

/// Generations are handed out once across every drawing, so a selection restored from a copy (a
/// cancelled command, a refused edit) brings back a generation nothing else has held since.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

impl Selection {
    /// Make `handles` the selection; the same handles again keep the generation.
    pub fn set(&mut self, handles: Vec<Handle>) {
        if handles != self.handles {
            self.generation = if handles.is_empty() { 0 } else { NEXT_GENERATION.fetch_add(1, Ordering::Relaxed) };
            self.handles = handles;
        }
    }

    /// Select nothing.
    pub fn clear(&mut self) {
        self.set(Vec::new());
    }

    /// A number that changes with the contents: equal generations hold equal handles, and 0 is
    /// the empty selection.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The handles, owned.
    pub fn into_vec(self) -> Vec<Handle> {
        self.handles
    }
}

impl From<Vec<Handle>> for Selection {
    fn from(handles: Vec<Handle>) -> Self {
        let mut s = Self::default();
        s.set(handles);
        s
    }
}

/// Selections are equal when they hold the same handles, whatever their generations.
impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        self.handles == other.handles
    }
}

impl Eq for Selection {}

impl Deref for Selection {
    type Target = [Handle];
    fn deref(&self) -> &[Handle] {
        &self.handles
    }
}

impl<'a> IntoIterator for &'a Selection {
    type Item = &'a Handle;
    type IntoIter = std::slice::Iter<'a, Handle>;
    fn into_iter(self) -> Self::IntoIter {
        self.handles.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generation_follows_the_contents() {
        let mut s = Selection::default();
        assert!(s.is_empty() && s.generation() == 0);
        s.set(vec![Handle(1), Handle(2)]);
        let g = s.generation();
        assert_ne!(g, 0);
        assert_eq!(&s[..], [Handle(1), Handle(2)]);
        assert_eq!(s.iter().count(), 2);

        // The same handles set again keep the generation: nothing to rebuild.
        s.set(vec![Handle(1), Handle(2)]);
        assert_eq!(s.generation(), g);
        s.set(vec![Handle(2), Handle(1)]);
        assert_ne!(s.generation(), g);

        // A copy keeps its generation with its handles, so restoring it hands both back.
        let kept = s.clone();
        s.clear();
        assert_eq!((s.len(), s.generation()), (0, 0));
        s = kept.clone();
        assert_eq!((s.generation(), &s[..]), (kept.generation(), &kept[..]));
        assert_eq!(s, kept);
        assert_eq!(Selection::from(vec![Handle(2), Handle(1)]), kept, "equal handles, whatever the generation");

        assert_eq!(Selection::from(Vec::new()).generation(), 0);
        assert_ne!(Selection::from(vec![Handle(3)]).generation(), Selection::from(vec![Handle(3)]).generation());
        assert_eq!(Selection::from(vec![Handle(3)]).into_vec(), [Handle(3)]);
    }
}

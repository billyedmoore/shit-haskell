//! The copying collector (Cheney's algorithm). Live objects are copied from the current semispace
//! into a fresh one; the roots are updated to the new addresses. Object layouts come from the
//! headers (`heap::pointer_fields`).

use super::heap::{self, kind, Heap, Word};

/// Collects `heap`, updating `roots` (the pointer words the caller holds). `need` is the number of
/// words the allocation that triggered the collection requires: the new space keeps that much room
/// (and a quarter of its size more), growing if the live data leaves too little.
pub fn collect(heap: &mut Heap, roots: &mut [Word], need: usize) {
    let mut space = std::mem::take(&mut heap.from);
    // The new space holds three times the data live at the last collection (as GHC sizes its old
    // generation at twice the live data), so that a large live set is not copied over and over.
    let mut capacity = space.len().max(heap.last_live * 3 + need);
    let pinned = std::mem::take(&mut heap.pinned);
    let bounds = (pinned.keys().min().copied().unwrap_or(1), pinned.keys().max().copied().unwrap_or(0));
    let mut reached: std::collections::HashSet<Word> = std::collections::HashSet::new();
    // The space left by the previous collection is reused when it is large enough: a fresh one
    // would be zeroed page by page.
    let spare = std::mem::take(&mut heap.to);
    let (mut to, mut live) = copy(&mut space, roots, capacity, &pinned, bounds, &mut reached, spare);
    capacity = to.len();
    while capacity - live < need + capacity / 4 {
        // Too little room: copy again into a larger space (the live data is all in `to`).
        capacity = (capacity * 2).max(live + need + capacity / 4);
        let (bigger, l) = copy(&mut to, roots, capacity, &pinned, bounds, &mut reached, Vec::new());
        to = bigger;
        live = l;
    }
    // The pinned objects nothing reached are freed.
    let mut kept = std::collections::HashMap::with_capacity(reached.len());
    for (p, (block, layout)) in pinned {
        if reached.contains(&p) {
            kept.insert(p, (block, layout));
        } else {
            // SAFETY: the block was allocated with this layout and nothing refers to it.
            unsafe { std::alloc::dealloc(block as *mut u8, layout) };
        }
    }
    heap.pinned = kept;
    heap.pinned_since_gc = 0;
    heap.last_live = live;
    heap.to = space;
    heap.from = to;
    heap.hp = live;
    heap.collections += 1;
    // HSC_GC_STATS: one line per collection.
    static STATS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *STATS.get_or_init(|| std::env::var_os("HSC_GC_STATS").is_some()) {
        eprintln!("gc {}: live {} words, space {} words, need {}", heap.collections, live, heap.from.len(), need);
    }
}

/// Copies everything reachable from `roots` out of `src` into a new space of `capacity` words
/// (`spare` when it is at least that large). Returns the new space and the number of live words.
/// The roots are updated in place; `src` is left with forwarding pointers.
fn copy(
    src: &mut [Word],
    roots: &mut [Word],
    capacity: usize,
    pinned: &std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
    reached: &mut std::collections::HashSet<Word>,
    spare: Vec<Word>,
) -> (Vec<Word>, usize) {
    let mut dst: Vec<Word> = if spare.len() >= capacity { spare } else { vec![0; capacity] };
    let mut free = 0usize;
    let base = src.as_ptr() as Word;
    let limit = base + (src.len() as Word) * 8;
    let pin = Pinned { map: pinned, bounds };
    for r in roots.iter_mut() {
        *r = evacuate(src, &mut dst, &mut free, *r, base, limit, &pin, reached);
    }
    // Cheney scan: the copied objects, in the order they were copied, give up their pointers.
    let mut scan = 0usize;
    while scan < free {
        let h = dst[scan];
        let total = heap::total_words(h);
        let (first, count) = heap::pointer_fields(h);
        for i in first..first + count {
            let v = dst[scan + i];
            // Pointers in `dst` are into `src` (the from-space), never into `dst`, until copied.
            dst[scan + i] = evacuate(src, &mut dst, &mut free, v, base, limit, &pin, reached);
        }
        scan += total;
    }
    (dst, free)
}

/// The pinned objects, with the range of their addresses (a quick test for most pointers).
struct Pinned<'a> {
    map: &'a std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
}

/// Copies the object `v` (if it is a pointer into the source space) into `dst`, leaving a
/// forwarding pointer in the source, and returns its new address. Anything else is unchanged.
#[allow(clippy::too_many_arguments)]
fn evacuate(
    src: &mut [Word],
    dst: &mut Vec<Word>,
    free: &mut usize,
    v: Word,
    base: Word,
    limit: Word,
    pinned: &Pinned,
    reached: &mut std::collections::HashSet<Word>,
) -> Word {
    if v < base || v >= limit {
        // A pinned object stays where it is (it holds no pointers); it is only marked as reached.
        if v >= pinned.bounds.0 && v <= pinned.bounds.1 && pinned.map.contains_key(&v) {
            reached.insert(v);
        }
        return v;
    }
    let from_index = ((v - base) / 8) as usize;
    let h = src[from_index];
    if heap::kind_of(h) == kind::FORWARD {
        return src[from_index + 1];
    }
    // An indirection (an updated thunk) is replaced by what it points to.
    if heap::kind_of(h) == kind::IND {
        let target = src[from_index + 1];
        if target != v {
            let new_addr = evacuate(src, dst, free, target, base, limit, pinned, reached);
            src[from_index] = kind::FORWARD as Word;
            src[from_index + 1] = new_addr;
            return new_addr;
        }
    }
    let total = heap::total_words(h);
    let new_index = *free;
    dst[new_index..new_index + total].copy_from_slice(&src[from_index..from_index + total]);
    *free += total;
    let new_addr = dst.as_ptr() as Word + (new_index as Word) * 8;
    src[from_index] = kind::FORWARD as Word;
    src[from_index + 1] = new_addr;
    new_addr
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cons cell list of `n` cells holding INT boxes, rooted at the head.
    fn build_list(heap: &mut Heap, n: usize) -> Word {
        let mut tail: Word = 0;
        for i in 0..n {
            let int = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
            heap.set_word(int, 1, i as Word);
            let cell = heap.try_alloc(3, heap::header(kind::CON, 2, 1)).unwrap();
            heap.set_word(cell, 1, int);
            heap.set_word(cell, 2, tail);
            tail = cell;
        }
        tail
    }

    fn sum_list(heap: &Heap, mut p: Word) -> u64 {
        let mut total = 0;
        while p != 0 {
            let int = heap.word(p, 1);
            total += heap.word(int, 1);
            p = heap.word(p, 2);
        }
        total
    }

    #[test]
    fn collection_keeps_reachable_data_and_drops_garbage() {
        let mut heap = Heap::new(4096);
        let mut root = build_list(&mut heap, 50);
        // Garbage: a list nobody references.
        let _garbage = build_list(&mut heap, 500);
        let before = heap.hp;
        let mut roots = [root];
        collect(&mut heap, &mut roots, 0);
        root = roots[0];
        assert!(heap.hp < before, "garbage was not reclaimed");
        assert_eq!(sum_list(&heap, root), (0..50u64).sum());
        assert_eq!(heap.collections, 1);
    }

    #[test]
    fn sharing_and_cycles_survive() {
        let mut heap = Heap::new(256);
        // A REF that points to itself.
        let r = heap.try_alloc(2, heap::header(kind::REF, 1, 0)).unwrap();
        heap.set_word(r, 1, r);
        let mut roots = [r];
        collect(&mut heap, &mut roots, 0);
        let r2 = roots[0];
        assert_eq!(heap.word(r2, 1), r2);
    }

    #[test]
    fn growth_when_the_new_space_is_too_small() {
        let mut heap = Heap::new(64);
        let root = build_list(&mut heap, 10);
        let mut roots = [root];
        collect(&mut heap, &mut roots, 1000);
        assert!(heap.from.len() >= 1000);
        assert_eq!(sum_list(&heap, roots[0]), (0..10u64).sum());
    }
}

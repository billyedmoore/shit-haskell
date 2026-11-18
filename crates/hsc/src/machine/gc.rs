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
    let mut c = Copier {
        src: src.as_mut_ptr(),
        base: src.as_ptr() as Word,
        span: (src.len() as Word) * 8,
        dst: dst.as_mut_ptr(),
        dst_base: dst.as_ptr() as Word,
        cap: dst.len(),
        free: 0,
        pinned: Pinned { map: pinned, bounds },
        reached,
    };
    // SAFETY: `src` and `dst` are distinct live buffers; every object copied lies in `src` (its
    // header gives its size) and `dst` has room for all live data (the caller sizes it and
    // `Copier::evacuate` checks).
    unsafe {
        for r in roots.iter_mut() {
            *r = c.evacuate(*r);
        }
        // Cheney scan: the copied objects, in the order they were copied, give up their pointers.
        let mut scan = 0usize;
        while scan < c.free {
            let obj = c.dst.add(scan);
            let h = *obj;
            let total = heap::total_words(h);
            let (first, count) = heap::pointer_fields(h);
            for i in first..first + count {
                let f = obj.add(i);
                // Pointers in `dst` are into `src` (the from-space), never into `dst`, until copied.
                *f = c.evacuate(*f);
            }
            scan += total;
        }
    }
    let free = c.free;
    (dst, free)
}

/// The pinned objects, with the range of their addresses (a quick test for most pointers).
struct Pinned<'a> {
    map: &'a std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
}

/// The state of one copy: the source space (`span` bytes from `base`), the destination and its
/// first free word.
struct Copier<'a> {
    src: *mut Word,
    base: Word,
    span: Word,
    dst: *mut Word,
    dst_base: Word,
    cap: usize,
    free: usize,
    pinned: Pinned<'a>,
    reached: &'a mut std::collections::HashSet<Word>,
}

impl Copier<'_> {
    /// Copies the object `v` (if it is a pointer into the source space) into the destination,
    /// leaving a forwarding pointer in the source, and returns its new address. Anything else is
    /// unchanged. An indirection (an updated thunk) is replaced by what it points to.
    ///
    /// # Safety
    /// As for `copy`.
    #[inline(always)]
    unsafe fn evacuate(&mut self, v: Word) -> Word {
        if v.wrapping_sub(self.base) >= self.span {
            // A pinned object stays where it is (it holds no pointers); it is only marked as reached.
            if v >= self.pinned.bounds.0 && v <= self.pinned.bounds.1 && self.pinned.map.contains_key(&v) {
                self.reached.insert(v);
            }
            return v;
        }
        let from = self.src.add(((v - self.base) / 8) as usize);
        let h = *from;
        let k = heap::kind_of(h);
        if k == kind::FORWARD {
            return *from.add(1);
        }
        if k == kind::IND && *from.add(1) != v {
            return self.indirection(from);
        }
        let total = heap::total_words(h);
        let at = self.free;
        assert!(at + total <= self.cap, "internal: collector out of room");
        let to = self.dst.add(at);
        match total {
            2 => {
                *to = h;
                *to.add(1) = *from.add(1);
            }
            3 => {
                *to = h;
                *to.add(1) = *from.add(1);
                *to.add(2) = *from.add(2);
            }
            _ => std::ptr::copy_nonoverlapping(from, to, total),
        }
        self.free = at + total;
        let new_addr = self.dst_base + (at as Word) * 8;
        *from = kind::FORWARD as Word;
        *from.add(1) = new_addr;
        new_addr
    }

    /// An indirection at `from`: what it points to is evacuated, and it forwards there.
    ///
    /// # Safety
    /// As for `copy`.
    #[inline(never)]
    unsafe fn indirection(&mut self, from: *mut Word) -> Word {
        let new_addr = self.evacuate(*from.add(1));
        *from = kind::FORWARD as Word;
        *from.add(1) = new_addr;
        new_addr
    }
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

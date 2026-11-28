//! The copying collector (Cheney's algorithm). Live objects are copied from the current semispace
//! into a fresh one; the roots are updated to the new addresses. Object layouts come from the
//! headers (`heap::pointer_fields`).

use super::heap::{self, kind, Heap, Word};

/// Header of an unevaluated single-capture thunk (the only thunks that can be selectors).
const THUNK1_HDR: Word = heap::header(kind::THUNK, 2, 0);

/// Open-addressed hash table mapping selector thunk code addresses to `(con_id, field_word)`,
/// where `field_word` is the 1-based word offset of the selected field in the constructor.
#[derive(Default)]
pub struct SelectorTable {
    /// `(code_addr, (con_id << 32) | field_word)`; `code_addr == 0` is an empty slot.
    slots: Vec<(Word, u64)>,
    mask: usize,
}

impl SelectorTable {
    pub fn new(entries: &[(Word, u32, u32)]) -> Self {
        if entries.is_empty() {
            return Self::default();
        }
        let cap = (entries.len() * 2).next_power_of_two().max(8);
        let mask = cap - 1;
        let mut slots = vec![(0 as Word, 0u64); cap];
        for &(code, con_id, field_word) in entries {
            if code == 0 {
                continue;
            }
            let packed = ((con_id as u64) << 32) | (field_word as u64);
            let mut idx = Self::hash(code) & mask;
            while slots[idx].0 != 0 && slots[idx].0 != code {
                idx = (idx + 1) & mask;
            }
            slots[idx] = (code, packed);
        }
        Self { slots, mask }
    }

    #[inline(always)]
    fn hash(code: Word) -> usize {
        (code.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 32) as usize
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    #[inline(always)]
    pub fn get(&self, code: Word) -> Option<(u32, u32)> {
        if self.slots.is_empty() {
            return None;
        }
        let mut idx = Self::hash(code) & self.mask;
        loop {
            // SAFETY: `idx <= self.mask < self.slots.len()` and the table is at most half full.
            let (k, v) = unsafe { *self.slots.get_unchecked(idx) };
            if k == code {
                return Some(((v >> 32) as u32, v as u32));
            }
            if k == 0 {
                return None;
            }
            idx = (idx + 1) & self.mask;
        }
    }
}

/// Collects `heap`, updating `roots` (the pointer words the caller holds). `need` is the number of
/// words the allocation that triggered the collection requires: the new space keeps that much room
/// (and a quarter of its size more), growing if the live data leaves too little.
pub fn collect(heap: &mut Heap, roots: &mut [Word], need: usize) {
    let empty = SelectorTable::default();
    collect_with_selectors(heap, roots, need, &empty, (0, 0), (0, 0));
}

/// Like `collect`, additionally short-circuiting selector thunks listed in `selectors` when their
/// captured scrutinee is already evaluated to the matching constructor (avoiding Wadler space leaks).
pub fn collect_with_selectors(
    heap: &mut Heap,
    roots: &mut [Word],
    need: usize,
    selectors: &SelectorTable,
    nursery: (Word, Word),
    survivors: (Word, Word),
) {
    let mut space = std::mem::take(&mut heap.from);
    // The new space holds three times the data live at the last collection (as GHC sizes its old
    // generation at twice the live data), growing geometrically so a growing heap does not
    // discard `spare` and page-fault a fresh semispace on every collection.
    let target = heap.last_live.saturating_mul(3).saturating_add(need);
    let mut capacity = if target > space.len() {
        space.len().saturating_mul(2).max(target.checked_next_power_of_two().unwrap_or(target))
    } else {
        space.len()
    };
    // Everything in the sources may be live: the old space's data and the young objects.
    let most = heap.hp + ((nursery.1 + survivors.1) / 8) as usize + need;
    if capacity < most {
        capacity = space.len().saturating_mul(2).max(most.checked_next_power_of_two().unwrap_or(most));
    }
    let pinned = std::mem::take(&mut heap.pinned);
    let bounds = (pinned.keys().min().copied().unwrap_or(1), pinned.keys().max().copied().unwrap_or(0));
    let mut reached: std::collections::HashSet<Word> = std::collections::HashSet::new();
    // The space left by the previous collection is reused when it is large enough: a fresh one
    // would be zeroed page by page.
    let spare = std::mem::take(&mut heap.to);
    let (mut to, mut live) =
        copy(&mut space, nursery, (survivors.0, survivors.1, 0), roots, capacity, &pinned, bounds, &mut reached, spare, selectors);
    capacity = to.len();
    while capacity - live < need + capacity / 4 {
        // Too little room: copy again into a larger space (the live data is all in `to`).
        let min_cap = (live + need).saturating_mul(4) / 3 + 1;
        capacity = capacity.saturating_mul(2).max(min_cap.checked_next_power_of_two().unwrap_or(min_cap));
        let (bigger, l) =
            copy(&mut to, (0, 0), (0, 0, 0), roots, capacity, &pinned, bounds, &mut reached, Vec::new(), selectors);
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

/// Copies everything reachable from `roots` out of `src` (and `extra`, a second source range
/// `(base, bytes)`, e.g. the nursery) into a new space of `capacity` words (`spare` when it is at
/// least that large). Returns the new space and the number of live words. The roots are updated
/// in place; the sources are left with forwarding pointers.
#[allow(clippy::too_many_arguments)]
fn copy(
    src: &mut [Word],
    extra: (Word, Word),
    extra2: (Word, Word, usize),
    roots: &mut [Word],
    capacity: usize,
    pinned: &std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
    reached: &mut std::collections::HashSet<Word>,
    spare: Vec<Word>,
    selectors: &SelectorTable,
) -> (Vec<Word>, usize) {
    let mut dst: Vec<Word> = if spare.len() >= capacity { spare } else { heap::space(capacity) };
    let sources = [(src.as_ptr() as Word, (src.len() as Word) * 8, 0), (extra.0, extra.1, 0), extra2];
    let target = Target { dst: dst.as_mut_ptr(), cap: dst.len(), free: 0 };
    // SAFETY: as for `copy_into`: `dst` is a distinct live buffer of `cap` words.
    let frees = unsafe {
        copy_into(sources, [target, target], roots, &[], (0, 0), &mut Vec::new(), pinned, bounds, reached, selectors)
    };
    (dst, frees[0])
}

/// Copies everything reachable from `roots` and from the pointer fields of the objects
/// `remembered` (old objects that may point into the sources) out of the `sources` ranges
/// (`(base, bytes, target)`) into their targets, after each target's `free` words. Objects that
/// end up outside the sources but point into `young` (the remembered objects, and those copied
/// into target 1 when it is the old space) are pushed to `still_young`. Returns the targets'
/// first free words.
///
/// # Safety
/// Each target is valid for `cap` words, distinct from the sources, and has room for its live
/// data (`Copier::copy_object` checks); every object reached in a source lies wholly in it, as
/// its header says; the remembered objects are live objects outside the sources.
#[allow(clippy::too_many_arguments)]
unsafe fn copy_into(
    sources: [(Word, Word, usize); 3],
    targets: [Target; 2],
    roots: &mut [Word],
    remembered: &[Word],
    young: (Word, Word),
    still_young: &mut Vec<Word>,
    pinned: &std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
    reached: &mut std::collections::HashSet<Word>,
    selectors: &SelectorTable,
) -> [usize; 2] {
    let shared = targets[0].dst == targets[1].dst;
    let mut c = Copier {
        src: [(sources[0].0, sources[0].1), (sources[1].0, sources[1].1), (sources[2].0, sources[2].1)],
        to: [sources[0].2, sources[1].2, sources[2].2],
        targets,
        force_old: false,
        pinned: Pinned { map: pinned, bounds },
        reached,
        selectors,
    };
    let is_young = |v: Word| v.wrapping_sub(young.0) < young.1;
    for r in roots.iter_mut() {
        *r = c.evacuate(*r);
    }
    // The remembered objects are old: what they reach is promoted directly (when the two
    // targets differ), as GHC promotes eagerly.
    c.force_old = !shared;
    for &obj in remembered {
        let o = obj as *mut Word;
        let h = *o;
        let (first, count) = heap::pointer_fields(h);
        let mut keep = false;
        for i in first..first + count {
            let f = o.add(i);
            *f = c.evacuate(*f);
            keep |= is_young(*f);
        }
        if keep {
            still_young.push(obj);
        }
    }
    // Cheney scan of each target: the copied objects, in the order they were copied, give up
    // their pointers (which may copy more objects into either target).
    let mut scan = [targets[0].free, targets[1].free];
    let ntargets = if shared { 1 } else { 2 };
    loop {
        let mut progressed = false;
        for t in 0..ntargets {
            c.force_old = t == 1 && !shared;
            while scan[t] < c.targets[t].free {
                progressed = true;
                let obj = c.targets[t].dst.add(scan[t]);
                let h = *obj;
                let total = heap::total_words(h);
                let (first, count) = heap::pointer_fields(h);
                let mut keep = false;
                for i in first..first + count {
                    let f = obj.add(i);
                    // The fields not scanned yet point into the sources (or outside them).
                    *f = c.evacuate(*f);
                    keep |= is_young(*f);
                }
                if t == 1 && keep {
                    still_young.push(obj as Word);
                }
                scan[t] += total;
            }
        }
        if !progressed {
            break;
        }
    }
    [c.targets[0].free, c.targets[1].free]
}

/// A minor collection. Live objects in the nursery (`nursery`: base and bytes used) move to the
/// survivor space `to_surv` (`to_cap` words); live objects in the survivor space `from_surv`
/// (base and bytes used), having survived a collection there, are promoted into the old space
/// after its live data. The roots are the stack's and so on, and the `remembered` old objects;
/// the old objects left pointing into the young space (`young`) are pushed to `still_young`.
/// Pinned objects are not freed (only a full collection knows which are unreachable). Answers
/// the survivor space's words used, or None, copying nothing, when the old space lacks room.
#[allow(clippy::too_many_arguments)]
pub fn collect_minor(
    heap: &mut Heap,
    nursery: (Word, Word),
    from_surv: (Word, Word),
    to_surv: (*mut Word, usize),
    young: (Word, Word),
    roots: &mut [Word],
    remembered: &[Word],
    still_young: &mut Vec<Word>,
    selectors: &SelectorTable,
) -> Option<usize> {
    let direct = heap.direct_left > 0;
    // Eager promotion may move nursery objects into the old space too.
    let room = ((from_surv.1 + nursery.1) / 8) as usize;
    if heap.from.len() - heap.hp < room {
        return None;
    }
    let pinned = std::mem::take(&mut heap.pinned);
    let bounds = (pinned.keys().min().copied().unwrap_or(1), pinned.keys().max().copied().unwrap_or(0));
    let mut reached = std::collections::HashSet::new();
    let start = heap.hp;
    let targets = [
        Target { dst: to_surv.0, cap: to_surv.1, free: 0 },
        Target { dst: heap.from.as_mut_ptr(), cap: heap.from.len(), free: start },
    ];
    // SAFETY: the survivor spaces and the old space are distinct; the to-survivor space holds a
    // whole nursery and the old space has room for the whole from-survivor space.
    let frees = unsafe {
        copy_into(
            [(nursery.0, nursery.1, direct as usize), (from_surv.0, from_surv.1, 1), (0, 0, 0)],
            targets,
            roots,
            remembered,
            young,
            still_young,
            &pinned,
            bounds,
            &mut reached,
            selectors,
        )
    };
    heap.pinned = pinned;
    heap.hp = frees[1];
    heap.minor_collections += 1;
    // When most of the nursery survives (a program building a large structure), ageing it in
    // the survivor space only copies it twice: the next collection promotes it directly.
    if direct {
        heap.direct_left -= 1;
    } else if 2 * frees[0] as Word > nursery.1 / 8 {
        heap.direct_left = 1;
    }
    static STATS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *STATS.get_or_init(|| std::env::var_os("HSC_GC_STATS").is_some()) {
        eprintln!(
            "minor gc {}: survivors {} words, promoted {} words, old {} of {} words, remembered {} -> {}",
            heap.minor_collections, frees[0], frees[1] - start, frees[1], heap.from.len(), remembered.len(), still_young.len()
        );
    }
    Some(frees[0])
}

/// The pinned objects, with the range of their addresses (a quick test for most pointers).
struct Pinned<'a> {
    map: &'a std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
}

/// A destination of a copy: `cap` words at `dst`, filled up to `free`.
#[derive(Clone, Copy)]
struct Target {
    dst: *mut Word,
    cap: usize,
    free: usize,
}

/// The state of one copy: the source ranges (`(base, bytes)`; empty ones have 0 bytes), the
/// destination each source's objects go to, and the destinations.
struct Copier<'a> {
    src: [(Word, Word); 3],
    to: [usize; 3],
    targets: [Target; 2],
    /// Eager promotion: while the fields of an old object are scanned, what they reach goes to
    /// target 1 (the old space), not the survivor space.
    force_old: bool,
    pinned: Pinned<'a>,
    reached: &'a mut std::collections::HashSet<Word>,
    selectors: &'a SelectorTable,
}

impl Copier<'_> {
    /// The target of the source range holding `v`, if any.
    #[inline(always)]
    fn target_of(&self, v: Word) -> Option<usize> {
        if v.wrapping_sub(self.src[0].0) < self.src[0].1 {
            Some(self.to[0])
        } else if v.wrapping_sub(self.src[1].0) < self.src[1].1 {
            Some(self.to[1])
        } else if v.wrapping_sub(self.src[2].0) < self.src[2].1 {
            Some(self.to[2])
        } else {
            None
        }
    }

    #[inline(always)]
    fn in_src(&self, v: Word) -> bool {
        self.target_of(v).is_some()
    }

    /// Copies the object `v` (if it is a pointer into the source space) into the destination,
    /// leaving a forwarding pointer in the source, and returns its new address. Anything else is
    /// unchanged. An indirection (an updated thunk) is replaced by what it points to.
    ///
    /// # Safety
    /// As for `copy`.
    #[inline(always)]
    unsafe fn evacuate(&mut self, v: Word) -> Word {
        let Some(t) = self.target_of(v).map(|t| t | self.force_old as usize) else {
            if v >= self.pinned.bounds.0 && v <= self.pinned.bounds.1 {
                self.pin(v);
            }
            return v;
        };
        let from = v as *mut Word;
        let h = *from;
        let k = heap::kind_of(h);
        if k == kind::FORWARD {
            return *from.add(1);
        }
        if k == kind::IND && *from.add(1) != v {
            return self.indirection(from);
        }
        if h == THUNK1_HDR && !self.selectors.is_empty() {
            if let Some(new_addr) = self.evacuate_selector(from) {
                return new_addr;
            }
        }
        self.copy_to(t, from, h)
    }

    #[inline(always)]
    unsafe fn copy_object(&mut self, from: *mut Word, h: Word) -> Word {
        let t = self.target_of(from as Word).expect("internal: copying a non-source object") | self.force_old as usize;
        self.copy_to(t, from, h)
    }

    #[inline(always)]
    unsafe fn copy_to(&mut self, t: usize, from: *mut Word, h: Word) -> Word {
        let total = heap::total_words(h);
        let t = &mut self.targets[t];
        let at = t.free;
        assert!(at + total <= t.cap, "internal: collector out of room");
        let to = t.dst.add(at);
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
            // Small objects are copied inline: a `memcpy` call costs more than the copy.
            4 => *(to as *mut [Word; 4]) = *(from as *const [Word; 4]),
            5 => *(to as *mut [Word; 5]) = *(from as *const [Word; 5]),
            6 => *(to as *mut [Word; 6]) = *(from as *const [Word; 6]),
            7 => *(to as *mut [Word; 7]) = *(from as *const [Word; 7]),
            8 => *(to as *mut [Word; 8]) = *(from as *const [Word; 8]),
            _ => std::ptr::copy_nonoverlapping(from, to, total),
        }
        t.free = at + total;
        let new_addr = t.dst as Word + (at as Word) * 8;
        *from = kind::FORWARD as Word;
        *from.add(1) = new_addr;
        new_addr
    }

    /// Short-circuits a chain of selector thunks (bounded depth, cycle-safe) when each thunk's
    /// captured scrutinee is already a matching `CON`. Returns `Some(new_addr)` if `from` was
    /// resolved and forwarded, or `None` to fall back to copying `from` normally.
    ///
    /// # Safety
    /// As for `copy`.
    #[inline(never)]
    unsafe fn evacuate_selector(&mut self, from: *mut Word) -> Option<Word> {
        let mut cur_ptr = from;
        let mut steps = 0u32;
        for _ in 0..16 {
            let code = *cur_ptr.add(1);
            let Some((con_id, field_word)) = self.selectors.get(code) else {
                break;
            };
            // Follow IND / FORWARD chains on the captured scrutinee pointer.
            let mut scrut = *cur_ptr.add(2);
            let mut matched: Option<Word> = None;
            for _ in 0..8 {
                if scrut == 0 {
                    break;
                }
                let sp = scrut as *const Word;
                let sh = *sp;
                let sk = heap::kind_of(sh);
                if sk == kind::FORWARD || (sk == kind::IND && *sp.add(1) != scrut) {
                    scrut = *sp.add(1);
                    continue;
                }
                if sk == kind::CON
                    && heap::aux_of(sh) == con_id
                    && field_word >= 1
                    && field_word <= heap::size_of(sh)
                {
                    matched = Some(*sp.add(field_word as usize));
                }
                break;
            }
            let Some(mut target) = matched else {
                break;
            };
            steps += 1;
            // Follow IND / FORWARD on the selected field (bounded to prevent IND cycles).
            let mut ind_ok = false;
            for _ in 0..8 {
                if !self.in_src(target) {
                    if target >= self.pinned.bounds.0 && target <= self.pinned.bounds.1 {
                        self.pin(target);
                    }
                    *from = kind::FORWARD as Word;
                    *from.add(1) = target;
                    return Some(target);
                }
                let next_ptr = target as *mut Word;
                if next_ptr == from {
                    // Direct or indirect cycle back to `from`: preserve the thunk for blackholing.
                    return None;
                }
                let nh = *next_ptr;
                let nk = heap::kind_of(nh);
                if nk == kind::FORWARD {
                    let new_addr = *next_ptr.add(1);
                    *from = kind::FORWARD as Word;
                    *from.add(1) = new_addr;
                    return Some(new_addr);
                }
                if nk == kind::IND && *next_ptr.add(1) != target {
                    target = *next_ptr.add(1);
                    continue;
                }
                if nh == THUNK1_HDR {
                    cur_ptr = next_ptr;
                    ind_ok = true;
                    break;
                }
                let new_addr = self.copy_object(next_ptr, nh);
                *from = kind::FORWARD as Word;
                *from.add(1) = new_addr;
                return Some(new_addr);
            }
            if !ind_ok {
                return None;
            }
        }
        // Only forward if we resolved at least one selector and stopped cleanly (not at depth 16).
        if steps > 0 && steps < 16 && cur_ptr != from {
            let new_addr = self.copy_object(cur_ptr, *cur_ptr);
            *from = kind::FORWARD as Word;
            *from.add(1) = new_addr;
            return Some(new_addr);
        }
        None
    }

    /// A pinned object stays where it is (it holds no pointers); it is only marked as reached.
    #[cold]
    fn pin(&mut self, v: Word) {
        if self.pinned.map.contains_key(&v) {
            self.reached.insert(v);
        }
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

    /// Allocates a 2-field CON in a raw buffer (a nursery) at word `*at`.
    fn con_in(buf: &mut [Word], at: &mut usize, a: Word, b: Word) -> Word {
        let p = buf.as_ptr() as Word + (*at as Word) * 8;
        buf[*at] = heap::header(kind::CON, 2, 1);
        buf[*at + 1] = a;
        buf[*at + 2] = b;
        *at += 3;
        p
    }

    #[test]
    fn minor_collection_ages_then_promotes_and_keeps_remembered_targets() {
        let mut heap = Heap::new(4096);
        heap.set_nursery(256, 256);
        let lo = heap.nursery.as_ptr() as Word;
        let young = (lo, (heap.nursery.len() as Word) * 8);
        // An old REF that points to a young cell (it was remembered by the write barrier).
        let old_ref = heap.try_alloc(2, heap::header(kind::REF, 1, 0)).unwrap();
        let mut at = 0;
        let leaf = con_in(&mut heap.nursery, &mut at, 0, 0);
        let kept = con_in(&mut heap.nursery, &mut at, leaf, 0);
        let rooted = con_in(&mut heap.nursery, &mut at, 0, 0);
        let _garbage = con_in(&mut heap.nursery, &mut at, 0, 0);
        heap.set_word(old_ref, 1, kept);
        let mut roots = [rooted];
        let mut still = Vec::new();
        let to = (heap.surv_base(1) as *mut Word, 256);
        let s0 = heap.surv_base(0);
        let surv = collect_minor(&mut heap, (lo, (at as Word) * 8), (s0, 0), to, young, &mut roots, &[old_ref], &mut still, &SelectorTable::default()).unwrap();
        // The remembered object's referents are promoted eagerly (into the old space); the
        // rooted cell ages in the survivor space; the garbage is gone.
        let kept2 = heap.word(old_ref, 1);
        assert!(heap.contains(kept2) && heap.contains(heap.word(kept2, 1)), "eagerly promoted");
        assert_eq!(surv, 3, "only the rooted cell is in the survivor space");
        assert!(roots[0].wrapping_sub(heap.surv_base(1)) < 256 * 8, "rooted cell aged");
        assert!(still.is_empty(), "nothing old points into the young space");
        // The next minor collection promotes the aged cell.
        let mut still = Vec::new();
        let to = (heap.surv_base(0) as *mut Word, 256);
        let s1 = heap.surv_base(1);
        let surv = collect_minor(&mut heap, (lo, 0), (s1, (surv as Word) * 8), to, young, &mut roots, &[], &mut still, &SelectorTable::default()).unwrap();
        assert_eq!(surv, 0);
        assert!(heap.contains(roots[0]), "aged cell promoted");
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

    #[test]
    fn indirections_are_shortened_and_shared_objects_copied_once() {
        let mut heap = Heap::new(4096);
        let int = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(int, 1, 42);
        // A chain of three indirections to the INT, and one that points to itself.
        let mut ind = int;
        for _ in 0..3 {
            let i = heap.try_alloc(2, heap::header(kind::IND, 1, 0)).unwrap();
            heap.set_word(i, 1, ind);
            ind = i;
        }
        let selfish = heap.try_alloc(2, heap::header(kind::IND, 1, 0)).unwrap();
        heap.set_word(selfish, 1, selfish);
        // A pair holding the chain twice, and the INT also as a root of its own.
        let pair = heap.try_alloc(3, heap::header(kind::CON, 2, 0)).unwrap();
        heap.set_word(pair, 1, ind);
        heap.set_word(pair, 2, ind);
        let mut roots = [pair, int, selfish, ind];
        collect(&mut heap, &mut roots, 0);
        let (pair, int, selfish, ind) = (roots[0], roots[1], roots[2], roots[3]);
        assert_eq!(ind, int, "an indirection root goes straight to its value");
        assert_eq!(heap.word(pair, 1), int);
        assert_eq!(heap.word(pair, 2), int);
        assert_eq!(heap.word(int, 1), 42);
        assert_eq!(heap.word(selfish, 1), selfish);
        // The pair, the INT and the self-indirection: 7 words, nothing copied twice.
        assert_eq!(heap.hp, 7);
    }

    #[test]
    fn selector_thunk_short_circuits_and_drops_dead_pair_half() {
        let mut heap = Heap::new(4096);
        let kept = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(kept, 1, 99);
        let dead_list = build_list(&mut heap, 200);
        // Pair (kept, dead_list) with con_id = 1.
        let pair = heap.try_alloc(3, heap::header(kind::CON, 2, 1)).unwrap();
        heap.set_word(pair, 1, kept);
        heap.set_word(pair, 2, dead_list);
        // Two chained selector thunks: outer selects field 1 of inner_pair, inner_pair selects field 1 of outer_pair.
        let outer_pair = heap.try_alloc(3, heap::header(kind::CON, 2, 1)).unwrap();
        heap.set_word(outer_pair, 1, pair);
        heap.set_word(outer_pair, 2, dead_list);
        let sel_code: Word = 0x1000_0000;
        let inner_sel = heap.try_alloc(3, THUNK1_HDR).unwrap();
        heap.set_word(inner_sel, 1, sel_code);
        heap.set_word(inner_sel, 2, outer_pair);
        let outer_sel = heap.try_alloc(3, THUNK1_HDR).unwrap();
        heap.set_word(outer_sel, 1, sel_code);
        heap.set_word(outer_sel, 2, inner_sel);

        let table = SelectorTable::new(&[(sel_code, 1, 1)]);
        // Wait: outer_sel captures inner_sel (a THUNK, not a CON yet), so let's test direct selector
        // on `pair` (resolved in 1 step) and chained `sel2 -> sel1 -> kept` where `pair` holds `sel1`.
        let sel1 = heap.try_alloc(3, THUNK1_HDR).unwrap();
        heap.set_word(sel1, 1, sel_code);
        heap.set_word(sel1, 2, pair);
        let pair2 = heap.try_alloc(3, heap::header(kind::CON, 2, 1)).unwrap();
        heap.set_word(pair2, 1, sel1);
        heap.set_word(pair2, 2, dead_list);
        let sel2 = heap.try_alloc(3, THUNK1_HDR).unwrap();
        heap.set_word(sel2, 1, sel_code);
        heap.set_word(sel2, 2, pair2);

        let _ = outer_sel;
        let mut roots = [sel2];
        collect_with_selectors(&mut heap, &mut roots, 0, &table, (0, 0), (0, 0));
        let r = roots[0];
        assert_eq!(heap::kind_of(heap.word(r, 0)), kind::INT);
        assert_eq!(heap.word(r, 1), 99);
        // Only the 2-word INT remains live: both pairs, both selector thunks, and the 200-element list were reclaimed!
        assert_eq!(heap.hp, 2);
    }

    #[test]
    fn selector_thunk_cycles_and_mismatched_cons_are_preserved() {
        let mut heap = Heap::new(4096);
        let sel_code: Word = 0x2000_0000;
        // Self-cycle: `a = fst (a, 1)`.
        let one = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(one, 1, 1);
        let cyc_pair = heap.try_alloc(3, heap::header(kind::CON, 2, 1)).unwrap();
        let cyc_sel = heap.try_alloc(3, THUNK1_HDR).unwrap();
        heap.set_word(cyc_sel, 1, sel_code);
        heap.set_word(cyc_sel, 2, cyc_pair);
        heap.set_word(cyc_pair, 1, cyc_sel);
        heap.set_word(cyc_pair, 2, one);
        // Constructor mismatch: selector expects con_id = 1, scrutinee has con_id = 7.
        let wrong_con = heap.try_alloc(3, heap::header(kind::CON, 2, 7)).unwrap();
        heap.set_word(wrong_con, 1, one);
        heap.set_word(wrong_con, 2, one);
        let wrong_sel = heap.try_alloc(3, THUNK1_HDR).unwrap();
        heap.set_word(wrong_sel, 1, sel_code);
        heap.set_word(wrong_sel, 2, wrong_con);

        let table = SelectorTable::new(&[(sel_code, 1, 1)]);
        let mut roots = [cyc_sel, wrong_sel];
        collect_with_selectors(&mut heap, &mut roots, 0, &table, (0, 0), (0, 0));
        assert_eq!(heap::kind_of(heap.word(roots[0], 0)), kind::THUNK);
        assert_eq!(heap::kind_of(heap.word(roots[1], 0)), kind::THUNK);
    }

    #[test]
    fn closure_raw_captures_are_preserved_verbatim_across_gc() {
        let mut heap = Heap::new(4096);
        let boxed = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(boxed, 1, 777);
        // Allocate a THUNK with 1 boxed capture + 2 trailing raw captures (total 5 words).
        // Put `boxed` itself (a real in-heap address!) as one of the raw words to verify GC does
        // NOT rewrite trailing raw captures as pointers.
        let hdr = heap::closure_header(kind::THUNK, 4, 2, 0);
        let thunk = heap.try_alloc(5, hdr).unwrap();
        heap.set_word(thunk, 1, 0x1234_5678);
        heap.set_word(thunk, 2, boxed);
        heap.set_word(thunk, 3, boxed); // raw word matching a heap address — must stay verbatim
        heap.set_word(thunk, 4, 0xdead_beef_cafe_babe);

        let mut roots = [thunk];
        collect(&mut heap, &mut roots, 0);
        let moved = roots[0];
        assert_ne!(moved, thunk);
        assert_eq!(heap.word(moved, 0), hdr);
        assert_eq!(heap.word(moved, 1), 0x1234_5678);
        let moved_boxed = heap.word(moved, 2);
        assert_ne!(moved_boxed, boxed);
        assert_eq!(heap.word(moved_boxed, 1), 777);
        assert_eq!(heap.word(moved, 3), boxed);
        assert_eq!(heap.word(moved, 4), 0xdead_beef_cafe_babe);
    }
}

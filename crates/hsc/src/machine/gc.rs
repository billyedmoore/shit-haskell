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
/// words the allocation that triggered the collection requires.
pub fn collect(heap: &mut Heap, roots: &mut [Word], need: usize) {
    let empty = SelectorTable::default();
    collect_with_selectors(heap, roots, need, &empty);
}

/// Like `collect`, additionally short-circuiting selector thunks listed in `selectors` when their
/// captured scrutinee is already evaluated to the matching constructor (avoiding Wadler space leaks).
pub fn collect_with_selectors(
    heap: &mut Heap,
    roots: &mut [Word],
    need: usize,
    selectors: &SelectorTable,
) {
    let locs: Vec<*mut Word> = roots.iter_mut().map(|r| r as *mut Word).collect();
    // SAFETY: `locs` points to the live root slice borrowed above.
    unsafe {
        if heap.needs_major() {
            collect_major(heap, &locs, need, selectors);
        } else {
            collect_minor(heap, &locs, &[], need, selectors);
        }
    }
}

/// Minor collection: evacuates live objects from `heap.nursery` into `heap.from[heap.old_hp..]`.
///
/// Precondition: `heap.from.len() - heap.old_hp >= heap.hp` so every live nursery object is
/// guaranteed to fit in `heap.from` without resizing.
///
/// # Safety
/// Every pointer in `root_locs` must point to a valid mutable root word disjoint from `heap`.
/// Each entry in `mut_thunks` must point to an old-generation indirection in `heap`.
pub unsafe fn collect_minor(
    heap: &mut Heap,
    root_locs: &[*mut Word],
    mut_thunks: &[Word],
    need: usize,
    selectors: &SelectorTable,
) {
    let nursery_used = heap.hp;
    debug_assert!(
        heap.from.len().saturating_sub(heap.old_hp) >= nursery_used,
        "minor GC requires room in old-gen for nursery_used"
    );
    let old_hp_before = heap.old_hp;
    let initial_old_muts = heap.old_muts.len();
    let dirty_arrays = heap.dirty_arrays.drain(..);
    let empty_pinned = std::collections::HashMap::new();
    let mut empty_reached = std::collections::HashSet::new();

    let mut c = Copier {
        src: MinorSrc {
            base: heap.nursery.as_ptr() as Word,
            span: (heap.nursery.len() as Word) * 8,
            src: heap.nursery.as_mut_ptr(),
        },
        dst: heap.from.as_mut_ptr(),
        dst_base: heap.from.as_ptr() as Word,
        cap: heap.from.len(),
        free: old_hp_before,
        old_muts: &mut heap.old_muts,
        pinned: Pinned { map: &empty_pinned, bounds: (1, 0) },
        reached: &mut empty_reached,
        selectors,
    };

    // SAFETY: `nursery` and `from` are distinct live buffers, `from` is not resized while copying,
    // and every root / remembered-set slot is a valid heap pointer slot.
    unsafe {
        for &loc in root_locs {
            *loc = c.evacuate(*loc);
        }
        for idx in 0..initial_old_muts {
            let obj = *c.old_muts.get_unchecked(idx) as *mut Word;
            let h = *obj;
            let (first, count) = heap::pointer_fields(h);
            for i in first..first + count {
                let f = obj.add(i);
                *f = c.evacuate(*f);
            }
        }
        for &t in mut_thunks {
            let f = (t as *mut Word).add(1);
            *f = c.evacuate(*f);
        }
        for a in dirty_arrays {
            let obj = a as *mut Word;
            let h = *obj & !(1u64 << 32);
            *obj = h;
            let count = heap::size_of(h) as usize;
            for i in 1..=count {
                let f = obj.add(i);
                *f = c.evacuate(*f);
            }
        }
        // Cheney scan of all objects promoted into `from[old_hp_before..c.free]`.
        c.cheney_scan(old_hp_before);
    }

    let promoted = c.free - old_hp_before;
    heap.old_hp = c.free;
    heap.hp = 0;
    heap.collections += 1;
    resize_nursery(heap, need);
    log_gc_stats(heap, false, promoted, need);
}

/// Major collection: evacuates live objects from BOTH `heap.from` and `heap.nursery` into a fresh
/// (or reused spare) old-generation space, frees unreachable pinned objects, and rebuilds `old_muts`.
///
/// # Safety
/// Every pointer in `root_locs` must point to a valid mutable root word disjoint from `heap`.
pub unsafe fn collect_major(
    heap: &mut Heap,
    root_locs: &[*mut Word],
    need: usize,
    selectors: &SelectorTable,
) {
    // Clear dirty bits on any mutated old arrays before copying.
    for a in heap.dirty_arrays.drain(..) {
        // SAFETY: every entry in `dirty_arrays` is a live ARRAY header in `heap.from`.
        unsafe { *(a as *mut Word) &= !(1u64 << 32) };
    }
    let nursery_used = heap.hp;
    let mut old_from = std::mem::take(&mut heap.from);
    // Normal promotions keep one nursery's worth of space in reserve. An oversized nursery
    // can still need a larger destination to fit both generations before we know what's live.
    let mut capacity = old_from.len().max(heap.old_hp.saturating_add(nursery_used));
    let pinned = std::mem::take(&mut heap.pinned);
    let bounds = (
        pinned.keys().min().copied().unwrap_or(1),
        pinned.keys().max().copied().unwrap_or(0),
    );
    let mut reached: std::collections::HashSet<Word> = std::collections::HashSet::new();
    let spare = std::mem::take(&mut heap.to);

    let (mut to, mut live, mut new_old_muts) = copy_major(
        &mut old_from,
        &mut heap.nursery,
        root_locs,
        capacity,
        &pinned,
        bounds,
        &mut reached,
        spare,
        selectors,
    );
    capacity = to.len();
    // Keep at least `live` free words (<= 50% occupancy) and room for at least 2 nurseries so we
    // don't immediately re-enter major GC.
    let min_free = live.max(heap.nursery_target.saturating_mul(2));
    while capacity.saturating_sub(live) < min_free {
        let min_cap = live.saturating_add(min_free);
        capacity = capacity
            .saturating_mul(2)
            .max(min_cap.checked_next_power_of_two().unwrap_or(min_cap));
        let (bigger, l, muts) = copy_old_only(
            &mut to,
            root_locs,
            capacity,
            &pinned,
            bounds,
            &mut reached,
            Vec::new(),
            selectors,
        );
        to = bigger;
        live = l;
        new_old_muts = muts;
    }

    // Free pinned objects that were not reached.
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
    heap.to = old_from;
    heap.from = to;
    heap.old_hp = live;
    heap.hp = 0;
    heap.old_muts = new_old_muts;
    heap.collections += 1;
    heap.major_collections += 1;
    resize_nursery(heap, need);
    log_gc_stats(heap, true, live, need);
}

/// Ensures `heap.nursery` has room for `need` words (temporarily growing for rare oversized
/// allocations, and shrinking back to `nursery_target` once the oversized request has passed).
fn resize_nursery(heap: &mut Heap, need: usize) {
    if need > heap.nursery.len() {
        let cap = need.saturating_add(need / 4).max(heap.nursery_target);
        heap.nursery = heap::space(cap);
    } else if heap.nursery.len() > heap.nursery_target && need <= heap.nursery_target {
        heap.nursery = heap::space(heap.nursery_target);
    }
}

fn log_gc_stats(heap: &Heap, major: bool, live_or_promoted: usize, need: usize) {
    static STATS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *STATS.get_or_init(|| std::env::var_os("HSC_GC_STATS").is_some()) {
        eprintln!(
            "gc {} ({}): live {} words, old {}/{} words, nursery {} words, need {}",
            heap.collections,
            if major { "major" } else { "minor" },
            live_or_promoted,
            heap.old_hp,
            heap.from.len(),
            heap.nursery.len(),
            need,
        );
    }
}

/// Copies everything reachable from `root_locs` out of `old_src` and `nursery_src` into a new
/// space of `capacity` words (`spare` when large enough).
#[allow(clippy::too_many_arguments)]
fn copy_major(
    old_src: &mut [Word],
    nursery_src: &mut [Word],
    root_locs: &[*mut Word],
    capacity: usize,
    pinned: &std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
    reached: &mut std::collections::HashSet<Word>,
    spare: Vec<Word>,
    selectors: &SelectorTable,
) -> (Vec<Word>, usize, Vec<Word>) {
    let mut dst: Vec<Word> = if spare.len() >= capacity { spare } else { heap::space(capacity) };
    let mut old_muts = Vec::new();
    let mut c = Copier {
        src: MajorSrc {
            old_base: old_src.as_ptr() as Word,
            old_span: (old_src.len() as Word) * 8,
            old_src: old_src.as_mut_ptr(),
            n_base: nursery_src.as_ptr() as Word,
            n_span: (nursery_src.len() as Word) * 8,
            n_src: nursery_src.as_mut_ptr(),
        },
        dst: dst.as_mut_ptr(),
        dst_base: dst.as_ptr() as Word,
        cap: dst.len(),
        free: 0,
        old_muts: &mut old_muts,
        pinned: Pinned { map: pinned, bounds },
        reached,
        selectors,
    };
    // SAFETY: `old_src`, `nursery_src`, and `dst` are distinct live buffers, `dst` is not resized.
    unsafe {
        for &loc in root_locs {
            *loc = c.evacuate(*loc);
        }
        c.cheney_scan(0);
    }
    let free = c.free;
    (dst, free, old_muts)
}

/// Copies everything reachable from `root_locs` out of a single `src` space (used when re-copying
/// into a larger old space after `copy_major` already emptied the nursery).
#[allow(clippy::too_many_arguments)]
fn copy_old_only(
    src: &mut [Word],
    root_locs: &[*mut Word],
    capacity: usize,
    pinned: &std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
    reached: &mut std::collections::HashSet<Word>,
    spare: Vec<Word>,
    selectors: &SelectorTable,
) -> (Vec<Word>, usize, Vec<Word>) {
    let mut dst: Vec<Word> = if spare.len() >= capacity { spare } else { heap::space(capacity) };
    let mut old_muts = Vec::new();
    let mut c = Copier {
        src: OldOnlySrc {
            base: src.as_ptr() as Word,
            span: (src.len() as Word) * 8,
            src: src.as_mut_ptr(),
        },
        dst: dst.as_mut_ptr(),
        dst_base: dst.as_ptr() as Word,
        cap: dst.len(),
        free: 0,
        old_muts: &mut old_muts,
        pinned: Pinned { map: pinned, bounds },
        reached,
        selectors,
    };
    // SAFETY: `src` and `dst` are distinct live buffers, `dst` is not resized.
    unsafe {
        for &loc in root_locs {
            *loc = c.evacuate(*loc);
        }
        c.cheney_scan(0);
    }
    let free = c.free;
    (dst, free, old_muts)
}

/// Zero-cost abstraction over the source space(s) being evacuated so minor and major GC share
/// identical evacuation/selector logic without runtime branching on GC mode.
trait SrcSpace {
    /// Whether pinned objects should be marked as reached during this collection.
    const TRACK_PINNED: bool;

    /// If `v` points inside a source space being evacuated, returns a pointer to its header word.
    unsafe fn resolve(&self, v: Word) -> Option<*mut Word>;
}

struct MinorSrc {
    base: Word,
    span: Word,
    src: *mut Word,
}

impl SrcSpace for MinorSrc {
    const TRACK_PINNED: bool = false;

    #[inline(always)]
    unsafe fn resolve(&self, v: Word) -> Option<*mut Word> {
        let off = v.wrapping_sub(self.base);
        if off < self.span {
            Some(self.src.add((off >> 3) as usize))
        } else {
            None
        }
    }
}

struct OldOnlySrc {
    base: Word,
    span: Word,
    src: *mut Word,
}

impl SrcSpace for OldOnlySrc {
    const TRACK_PINNED: bool = true;

    #[inline(always)]
    unsafe fn resolve(&self, v: Word) -> Option<*mut Word> {
        let off = v.wrapping_sub(self.base);
        if off < self.span {
            Some(self.src.add((off >> 3) as usize))
        } else {
            None
        }
    }
}

struct MajorSrc {
    old_base: Word,
    old_span: Word,
    old_src: *mut Word,
    n_base: Word,
    n_span: Word,
    n_src: *mut Word,
}

impl SrcSpace for MajorSrc {
    const TRACK_PINNED: bool = true;

    #[inline(always)]
    unsafe fn resolve(&self, v: Word) -> Option<*mut Word> {
        let off = v.wrapping_sub(self.old_base);
        if off < self.old_span {
            return Some(self.old_src.add((off >> 3) as usize));
        }
        let noff = v.wrapping_sub(self.n_base);
        if noff < self.n_span {
            return Some(self.n_src.add((noff >> 3) as usize));
        }
        None
    }
}

/// The pinned objects, with the range of their addresses (a quick test for most pointers).
struct Pinned<'a> {
    map: &'a std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    bounds: (Word, Word),
}

/// The state of one copy: the source space(s), the destination and its first free word.
struct Copier<'a, S: SrcSpace> {
    src: S,
    dst: *mut Word,
    dst_base: Word,
    cap: usize,
    free: usize,
    old_muts: &'a mut Vec<Word>,
    pinned: Pinned<'a>,
    reached: &'a mut std::collections::HashSet<Word>,
    selectors: &'a SelectorTable,
}

impl<S: SrcSpace> Copier<'_, S> {
    /// Cheney scan: walks objects copied into `dst[start..self.free]` and evacuates their pointers.
    #[inline(always)]
    unsafe fn cheney_scan(&mut self, mut scan: usize) {
        while scan < self.free {
            let obj = self.dst.add(scan);
            let h = *obj;
            let total = heap::total_words(h);
            let (first, count) = heap::pointer_fields(h);
            for i in first..first + count {
                let f = obj.add(i);
                *f = self.evacuate(*f);
            }
            scan += total;
        }
    }

    /// Copies the object `v` (if it is a pointer into a source space) into the destination,
    /// leaving a forwarding pointer in the source, and returns its new address. Anything else is
    /// unchanged. An indirection (an updated thunk) is replaced by what it points to.
    #[inline(always)]
    unsafe fn evacuate(&mut self, v: Word) -> Word {
        let Some(from) = self.src.resolve(v) else {
            if S::TRACK_PINNED && v >= self.pinned.bounds.0 && v <= self.pinned.bounds.1 {
                self.pin(v);
            }
            return v;
        };
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
        self.copy_object(from, h)
    }

    #[inline(always)]
    unsafe fn copy_object(&mut self, from: *mut Word, h: Word) -> Word {
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
            4 => {
                *to = h;
                *to.add(1) = *from.add(1);
                *to.add(2) = *from.add(2);
                *to.add(3) = *from.add(3);
            }
            5 => {
                *to = h;
                *to.add(1) = *from.add(1);
                *to.add(2) = *from.add(2);
                *to.add(3) = *from.add(3);
                *to.add(4) = *from.add(4);
            }
            _ => std::ptr::copy_nonoverlapping(from, to, total),
        }
        self.free = at + total;
        let new_addr = self.dst_base + (at as Word) * 8;
        let k = heap::kind_of(h);
        if k >= kind::REF && (k == kind::REF || k == kind::MVAR || k == kind::TVAR) {
            self.old_muts.push(new_addr);
        }
        *from = kind::FORWARD as Word;
        *from.add(1) = new_addr;
        new_addr
    }

    /// Short-circuits a chain of selector thunks (bounded depth, cycle-safe) when each thunk's
    /// captured scrutinee is already a matching `CON`. Returns `Some(new_addr)` if `from` was
    /// resolved and forwarded, or `None` to fall back to copying `from` normally.
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
                let Some(next_ptr) = self.src.resolve(target) else {
                    if S::TRACK_PINNED
                        && target >= self.pinned.bounds.0
                        && target <= self.pinned.bounds.1
                    {
                        self.pin(target);
                    }
                    *from = kind::FORWARD as Word;
                    *from.add(1) = target;
                    return Some(target);
                };
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
        assert!(heap.old_hp < before, "garbage was not reclaimed");
        assert_eq!(heap.hp, 0, "nursery should be empty after GC");
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
        assert!(heap.old_muts.contains(&r2));
    }

    #[test]
    fn growth_when_the_new_space_is_too_small() {
        let mut heap = Heap::new(64);
        let root = build_list(&mut heap, 10);
        let mut roots = [root];
        collect(&mut heap, &mut roots, 1000);
        assert!(heap.nursery.len() >= 1000);
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
        assert_eq!(heap.old_hp, 7);
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
        let sel_code: Word = 0x1000_0000;
        let table = SelectorTable::new(&[(sel_code, 1, 1)]);
        let sel1 = heap.try_alloc(3, THUNK1_HDR).unwrap();
        heap.set_word(sel1, 1, sel_code);
        heap.set_word(sel1, 2, pair);
        let pair2 = heap.try_alloc(3, heap::header(kind::CON, 2, 1)).unwrap();
        heap.set_word(pair2, 1, sel1);
        heap.set_word(pair2, 2, dead_list);
        let sel2 = heap.try_alloc(3, THUNK1_HDR).unwrap();
        heap.set_word(sel2, 1, sel_code);
        heap.set_word(sel2, 2, pair2);

        let mut roots = [sel2];
        collect_with_selectors(&mut heap, &mut roots, 0, &table);
        let r = roots[0];
        assert_eq!(heap::kind_of(heap.word(r, 0)), kind::INT);
        assert_eq!(heap.word(r, 1), 99);
        // Only the 2-word INT remains live: both pairs, both selector thunks, and the 200-element list were reclaimed!
        assert_eq!(heap.old_hp, 2);
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
        collect_with_selectors(&mut heap, &mut roots, 0, &table);
        assert_eq!(heap::kind_of(heap.word(roots[0], 0)), kind::THUNK);
        assert_eq!(heap::kind_of(heap.word(roots[1], 0)), kind::THUNK);
    }

    #[test]
    fn minor_gc_promotes_old_ref_mvar_array_and_thunk_ind_edges() {
        let mut heap = Heap::new_generational(256, 1024);
        let empty_sel = SelectorTable::default();
        // Allocate REF, MVAR, ARRAY, and THUNK in nursery, then promote them via minor GC.
        let zero = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(zero, 1, 0);
        let old_ref = heap.try_alloc(2, heap::header(kind::REF, 1, 0)).unwrap();
        heap.set_word(old_ref, 1, zero);
        let old_mvar = heap.try_alloc(2, heap::header(kind::MVAR, 1, 7)).unwrap();
        heap.set_word(old_mvar, 1, 0);
        let old_arr = heap.try_alloc(3, heap::header(kind::ARRAY, 2, 0)).unwrap();
        heap.set_word(old_arr, 1, zero);
        heap.set_word(old_arr, 2, zero);
        let old_thunk = heap.try_alloc_old(2, heap::header(kind::THUNK, 1, 0)).unwrap();
        heap.set_word(old_thunk, 1, 0x1234);

        let mut roots = [old_ref, old_mvar, old_arr];
        let locs: Vec<*mut Word> = roots.iter_mut().map(|r| r as *mut Word).collect();
        unsafe { collect_minor(&mut heap, &locs, &[], 0, &empty_sel) };
        let (old_ref, old_mvar, old_arr) = (roots[0], roots[1], roots[2]);
        assert!(heap.in_old(old_ref) && heap.in_old(old_mvar) && heap.in_old(old_arr));
        assert_eq!(heap.old_muts.len(), 2);

        // Now allocate young objects in nursery and point old_ref, old_mvar, old_arr, and old_thunk at them!
        let v_ref = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(v_ref, 1, 111);
        let v_mvar = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(v_mvar, 1, 222);
        let v_arr = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(v_arr, 1, 333);
        let v_ind = heap.try_alloc(2, heap::header(kind::INT, 1, 0)).unwrap();
        heap.set_word(v_ind, 1, 444);

        heap.set_word(old_ref, 1, v_ref);
        heap.set_word(old_mvar, 1, v_mvar);
        heap.set_word(old_arr, 2, v_arr);
        heap.set_word(old_arr, 0, heap.word(old_arr, 0) | (1u64 << 32));
        heap.dirty_arrays.push(old_arr);
        heap.set_word(old_thunk, 0, heap::header(kind::IND, 1, 0));
        heap.set_word(old_thunk, 1, v_ind);

        // Run minor GC with NO stack roots referencing the young values or old_thunk (simulating global CAF).
        unsafe { collect_minor(&mut heap, &[], &[old_thunk], 0, &empty_sel) };
        assert_eq!(heap.major_collections, 0);
        // Dirty bit on old_arr must be cleared.
        assert_eq!(heap::aux_of(heap.word(old_arr, 0)), 0);
        // Every old-to-young target must now live in old-gen with the right payload!
        let r_val = heap.word(old_ref, 1);
        let m_val = heap.word(old_mvar, 1);
        let a_val = heap.word(old_arr, 2);
        let t_val = heap.word(old_thunk, 1);
        assert!(heap.in_old(r_val) && heap.word(r_val, 1) == 111);
        assert!(heap.in_old(m_val) && heap.word(m_val, 1) == 222);
        assert!(heap.in_old(a_val) && heap.word(a_val, 1) == 333);
        assert!(heap.in_old(t_val) && heap.word(t_val, 1) == 444);

        // Now drop old_mvar and run major GC: old_muts must shrink to 1 (only old_ref) and old_thunk IND shortens!
        let mut major_roots = [old_ref, old_arr, old_thunk];
        let mlocs: Vec<*mut Word> = major_roots.iter_mut().map(|r| r as *mut Word).collect();
        unsafe { collect_major(&mut heap, &mlocs, 0, &empty_sel) };
        assert_eq!(heap.major_collections, 1);
        assert_eq!(heap.old_muts.len(), 1);
        assert_eq!(heap.old_muts[0], major_roots[0]);
        assert_eq!(heap.word(heap.word(major_roots[0], 1), 1), 111);
        assert_eq!(heap.word(heap.word(major_roots[1], 2), 1), 333);
        assert_eq!(heap::kind_of(heap.word(major_roots[2], 0)), kind::INT);
        assert_eq!(heap.word(major_roots[2], 1), 444);
    }

    #[test]
    fn pinned_objects_survive_minor_and_reclaim_on_major() {
        let mut heap = Heap::new_generational(128, 512);
        let empty_sel = SelectorTable::default();
        let kept_pin = heap.alloc_pinned(4, 16, heap::header(kind::BYTES, 3, 24));
        let dead_pin = heap.alloc_pinned(4, 16, heap::header(kind::BYTES, 3, 24));
        let mut roots = [kept_pin];
        let locs: Vec<*mut Word> = roots.iter_mut().map(|r| r as *mut Word).collect();
        // Minor GC does not free pinned objects.
        unsafe { collect_minor(&mut heap, &locs, &[], 0, &empty_sel) };
        assert!(heap.is_pinned(kept_pin) && heap.is_pinned(dead_pin));
        // Major GC keeps `kept_pin` and frees `dead_pin`.
        unsafe { collect_major(&mut heap, &locs, 0, &empty_sel) };
        assert!(heap.is_pinned(kept_pin));
        assert!(!heap.is_pinned(dead_pin));
    }

    #[test]
    fn repeated_promotions_of_short_lived_objects_do_not_grow_the_old_space() {
        let mut heap = Heap::new_generational(32, 128);
        for _ in 0..20 {
            let root = build_list(&mut heap, 6);
            let mut roots = [root];
            collect(&mut heap, &mut roots, 0);
            assert_eq!(sum_list(&heap, roots[0]), (0..6u64).sum());
        }
        assert!(heap.major_collections > 1);
        assert_eq!(heap.from.len(), 128);
    }
}

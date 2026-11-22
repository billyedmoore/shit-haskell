//! The object model of the native machine: heap objects are runs of 64-bit words. Word 0 is the
//! header, `kind:8 | size:24 | aux:32`, where size counts the words after the header. Every value a
//! program can hold is a pointer to such an object; raw numbers are objects with a raw payload.
//!
//! The collector (`gc`) promotes nursery survivors into the old generation and compacts both
//! generations on a major collection. Generated code spills live pointers to the Haskell stack
//! or the machine record so the collector can update them.

/// Object kinds (the low byte of the header).
pub mod kind {
    pub const CON: u8 = 1;
    pub const INT: u8 = 2;
    pub const DOUBLE: u8 = 3;
    pub const CHAR: u8 = 4;
    pub const INTEGER: u8 = 5;
    pub const FUN: u8 = 6;
    pub const PAP: u8 = 7;
    pub const THUNK: u8 = 8;
    pub const IND: u8 = 9;
    pub const BLACKHOLE: u8 = 10;
    pub const REF: u8 = 11;
    pub const ARRAY: u8 = 12;
    pub const BYTES: u8 = 13;
    pub const ADDR: u8 = 14;
    pub const MVAR: u8 = 15;
    pub const TVAR: u8 = 16;
    /// A forwarding pointer left behind by the collector: word 1 is the new address.
    pub const FORWARD: u8 = 255;
}

/// A heap word, or a pointer to an object (its address).
pub type Word = u64;

/// Packs a header.
pub const fn header(kind: u8, size: u32, aux: u32) -> Word {
    debug_assert!(size < (1 << 24), "object too large");
    kind as u64 | ((size as u64) << 8) | ((aux as u64) << 32)
}

pub fn kind_of(h: Word) -> u8 {
    (h & 0xff) as u8
}

/// The number of words after the header.
pub fn size_of(h: Word) -> u32 {
    ((h >> 8) & 0xff_ffff) as u32
}

pub fn aux_of(h: Word) -> u32 {
    (h >> 32) as u32
}

/// The total number of words of an object, header included. Every object has at least two words,
/// so that a forwarding pointer or an indirection fits in place.
pub fn total_words(h: Word) -> usize {
    (size_of(h) as usize + 1).max(2)
}

/// Pointer-field layout of an object: the word range of its pointer fields, given its header.
/// Returns (first field word index, count of pointer words). Words outside are raw.
pub fn pointer_fields(h: Word) -> (usize, usize) {
    let n = size_of(h) as usize;
    match kind_of(h) {
        kind::CON | kind::ARRAY | kind::REF => (1, n),
        // word 1 is the code entry (raw); the rest are free variables
        kind::FUN | kind::THUNK => (2, n.saturating_sub(1)),
        // word 1 is the function object, the rest are its arguments
        kind::PAP => (1, n),
        kind::IND => (1, 1),
        // a variable holds one pointer (null when empty)
        kind::MVAR | kind::TVAR => (1, 1),
        kind::ADDR => (1, 1),
        _ => (0, 0),
    }
}

#[cfg(target_os = "linux")]
extern "C" {
    fn madvise(addr: *mut u8, len: usize, advice: i32) -> i32;
}

/// A zeroed semispace of `words` words. On Linux a large one is backed by huge pages where the
/// kernel allows them on request: allocation streams through the whole space, and with 4 KB
/// pages its first pass is mostly page faults.
pub fn space(words: usize) -> Vec<Word> {
    #[allow(unused_mut)]
    let mut v: Vec<Word> = vec![0; words];
    #[cfg(target_os = "linux")]
    {
        const MADV_HUGEPAGE: i32 = 14;
        const HUGE: usize = 2 << 20;
        let bytes = words * 8;
        if bytes >= 3 * HUGE {
            let start = v.as_mut_ptr() as usize;
            // Leave the first 2 MB on 4 KB pages so small programs touching only a few KB at
            // startup do not pay a 2 MB huge-page zero-fault.
            let aligned = (start + 2 * HUGE - 1) & !(HUGE - 1);
            let end = (start + bytes) & !(HUGE - 1);
            if end > aligned {
                // SAFETY: a range inside the vector's allocation, which stays the allocator's to
                // free or reuse: the advice changes no contents and only affects how the kernel
                // backs those pages.
                unsafe { madvise(aligned as *mut u8, end - aligned, MADV_HUGEPAGE) };
            }
        }
    }
    v
}

/// A two-generation copying heap: a small L2-hot nursery (`nursery`) for fast bump allocation,
/// and an old-generation semispace pair (`from` / `to`) where survivors are promoted and compacted.
pub struct Heap {
    /// The young generation (nursery): generated code and `m.alloc` bump-allocate here.
    pub nursery: Vec<Word>,
    /// Normal target size of `nursery` in words (oversized nurseries shrink back to this on GC).
    pub nursery_target: usize,
    /// Next free word of `nursery`, as an index.
    pub hp: usize,
    /// The old-generation space holding promoted objects and startup globals.
    pub from: Vec<Word>,
    /// The old-generation spare space for major collections (empty between collections).
    pub to: Vec<Word>,
    /// Next free word of `from`, as an index.
    pub old_hp: usize,
    /// All `REF`, `MVAR`, and `TVAR` objects currently in `from` (scanned on minor GC; rebuilt on major GC).
    pub old_muts: Vec<Word>,
    /// Old-generation `ARRAY` objects mutated since the last GC (aux bit 0 set while queued).
    pub dirty_arrays: Vec<Word>,
    /// Total collections (minor + major) so far.
    pub collections: u64,
    /// Major collections so far.
    pub major_collections: u64,
    /// Pinned objects (byte arrays the collector never moves, so that C code and raw addresses
    /// may refer to their bytes): each object's address and the block it was allocated in. They
    /// live outside the semispaces; a major collection frees the ones it did not reach.
    pub pinned: std::collections::HashMap<Word, (usize, std::alloc::Layout)>,
    /// Bytes of pinned objects allocated since the last major collection.
    pub pinned_since_gc: usize,
}

impl Heap {
    /// Creates a heap with equal nursery and initial old-gen sizes (convenient for unit tests).
    pub fn new(words: usize) -> Heap {
        Heap::new_generational(words, words)
    }

    /// Creates a two-generation heap with `nursery_words` young space and `old_words` initial old space.
    pub fn new_generational(nursery_words: usize, old_words: usize) -> Heap {
        let nursery_words = nursery_words.max(16);
        let old_words = old_words.max(nursery_words);
        Heap {
            nursery: space(nursery_words),
            nursery_target: nursery_words,
            hp: 0,
            from: space(old_words),
            to: Vec::new(),
            old_hp: 0,
            old_muts: Vec::new(),
            dirty_arrays: Vec::new(),
            collections: 0,
            major_collections: 0,
            pinned: std::collections::HashMap::new(),
            pinned_since_gc: 0,
        }
    }

    /// Allocates a pinned object of `total` words (header included) whose second word (the data)
    /// is aligned to `align` bytes; it is zeroed and given header `h`.
    pub fn alloc_pinned(&mut self, total: usize, align: usize, h: Word) -> Word {
        let align = align.max(16).next_power_of_two();
        let size = (total.max(2) * 8 + align).max(16);
        let layout = std::alloc::Layout::from_size_align(size, align).expect("pinned layout");
        // SAFETY: the layout has a non-zero size.
        let block = unsafe { std::alloc::alloc_zeroed(layout) };
        if block.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        // The header sits just before an aligned data address.
        let p = block as usize + align - 8;
        self.pinned.insert(p as Word, (block as usize, layout));
        self.pinned_since_gc += size;
        self.set_word(p as Word, 0, h);
        p as Word
    }

    /// Whether `p` is a pinned object.
    pub fn is_pinned(&self, p: Word) -> bool {
        self.pinned.contains_key(&p)
    }

    /// The address of word `i` of the nursery, as a pointer value.
    #[inline(always)]
    pub fn nursery_addr(&self, i: usize) -> Word {
        self.nursery.as_ptr() as Word + (i as Word) * 8
    }

    /// The address of word `i` of the old-generation space, as a pointer value.
    #[inline(always)]
    pub fn old_addr(&self, i: usize) -> Word {
        self.from.as_ptr() as Word + (i as Word) * 8
    }

    /// Alias for `old_addr` (for diagnostics and tests).
    #[inline(always)]
    pub fn addr(&self, i: usize) -> Word {
        self.old_addr(i)
    }

    /// Whether `p` is an object address in the nursery.
    #[inline(always)]
    pub fn in_nursery(&self, p: Word) -> bool {
        let base = self.nursery.as_ptr() as Word;
        p.wrapping_sub(base) < (self.nursery.len() as Word) * 8
    }

    /// Whether `p` is an object address in the old-generation space.
    #[inline(always)]
    pub fn in_old(&self, p: Word) -> bool {
        let base = self.from.as_ptr() as Word;
        p.wrapping_sub(base) < (self.from.len() as Word) * 8
    }

    /// Leave enough old-generation space to collect the next full nursery without growing the
    /// old semispace just to accommodate garbage that a major collection will discard.
    pub fn needs_major(&self) -> bool {
        self.from.len() - self.old_hp < self.hp.saturating_add(self.nursery_target)
    }

    /// Whether `p` is an object address in either the nursery or old-generation space.
    pub fn contains(&self, p: Word) -> bool {
        self.in_nursery(p) || self.in_old(p)
    }

    /// The word index of an address in the old-generation space.
    pub fn index(&self, p: Word) -> usize {
        ((p - self.from.as_ptr() as Word) / 8) as usize
    }

    /// Reads word `i` of an object at pointer `p`.
    pub fn word(&self, p: Word, i: usize) -> Word {
        // SAFETY: callers pass pointers of live objects of this heap; the bounds are checked.
        unsafe { *((p as *const Word).add(i)) }
    }

    /// Writes word `i` of an object at pointer `p`.
    pub fn set_word(&self, p: Word, i: usize, v: Word) {
        // SAFETY: as for `word`; the write is within the object.
        unsafe { *((p as *mut Word).add(i)) = v }
    }

    /// Allocates an object of `total` words (header included) in the nursery with the given
    /// header, returning its address, or None when the nursery is full.
    pub fn try_alloc(&mut self, total: usize, h: Word) -> Option<Word> {
        if self.hp + total > self.nursery.len() {
            return None;
        }
        let p = self.nursery_addr(self.hp);
        self.hp += total;
        self.set_word(p, 0, h);
        Some(p)
    }

    /// Allocates an object directly in the old-generation space (for startup globals and tests).
    pub fn try_alloc_old(&mut self, total: usize, h: Word) -> Option<Word> {
        if self.old_hp + total > self.from.len() {
            return None;
        }
        let p = self.old_addr(self.old_hp);
        self.old_hp += total;
        self.set_word(p, 0, h);
        let k = kind_of(h);
        if k == kind::REF || k == kind::MVAR || k == kind::TVAR {
            self.old_muts.push(p);
        }
        Some(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip() {
        let h = header(kind::FUN, 5, 3);
        assert_eq!(kind_of(h), kind::FUN);
        assert_eq!(size_of(h), 5);
        assert_eq!(aux_of(h), 3);
        assert_eq!(total_words(header(kind::INT, 0, 0)), 2);
    }

    #[test]
    fn allocation_bumps_and_fills() {
        let mut heap = Heap::new(16);
        let a = heap.try_alloc(6, header(kind::CON, 5, 7)).unwrap();
        let b = heap.try_alloc(6, header(kind::CON, 5, 8)).unwrap();
        assert_eq!(heap.word(a, 0), header(kind::CON, 5, 7));
        assert_eq!(heap.word(b, 0), header(kind::CON, 5, 8));
        assert!(heap.try_alloc(6, header(kind::CON, 5, 9)).is_none());
        assert!(heap.contains(a) && heap.contains(b));
        assert!(heap.in_nursery(a) && !heap.in_old(a));
    }
}

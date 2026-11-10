//! The object model of the native machine: heap objects are runs of 64-bit words. Word 0 is the
//! header, `kind:8 | size:24 | aux:32`, where size counts the words after the header. Every value a
//! program can hold is a pointer to such an object; raw numbers are objects with a raw payload.
//!
//! The collector (`gc`) is a copying collector over two semispaces. Objects move, so generated code
//! keeps its live values only on the Haskell stack, where the collector finds them.

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
pub fn header(kind: u8, size: u32, aux: u32) -> Word {
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

/// A heap with two semispaces. Allocation is by bumping `hp` towards `limit`.
pub struct Heap {
    /// The space objects are allocated in.
    pub from: Vec<Word>,
    /// The space the collector copies into (empty between collections).
    pub to: Vec<Word>,
    /// Next free word of `from`, as an index.
    pub hp: usize,
    /// Collections so far.
    pub collections: u64,
}

impl Heap {
    pub fn new(words: usize) -> Heap {
        Heap { from: vec![0; words], to: Vec::new(), hp: 0, collections: 0 }
    }

    /// The address of word `i` of the current space, as a pointer value.
    pub fn addr(&self, i: usize) -> Word {
        self.from.as_ptr() as Word + (i as Word) * 8
    }

    /// Whether `p` is an object address in the current space.
    pub fn contains(&self, p: Word) -> bool {
        let base = self.from.as_ptr() as Word;
        p >= base && p < base + (self.from.len() as Word) * 8
    }

    /// The word index of an address in the current space.
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

    /// Allocates an object of `total` words (header included) with the given header, returning its
    /// address, or None when the space is full (the caller collects and retries).
    pub fn try_alloc(&mut self, total: usize, h: Word) -> Option<Word> {
        if self.hp + total > self.from.len() {
            return None;
        }
        let p = self.addr(self.hp);
        self.hp += total;
        self.set_word(p, 0, h);
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
        let mut heap = Heap::new(8);
        let a = heap.try_alloc(3, header(kind::CON, 2, 7)).unwrap();
        let b = heap.try_alloc(3, header(kind::CON, 2, 8)).unwrap();
        assert_eq!(heap.word(a, 0), header(kind::CON, 2, 7));
        assert_eq!(heap.word(b, 0), header(kind::CON, 2, 8));
        assert!(heap.try_alloc(3, header(kind::CON, 2, 9)).is_none());
        assert!(heap.contains(a) && heap.contains(b));
    }
}

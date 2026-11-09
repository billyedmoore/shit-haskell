#!/usr/bin/env python3
"""One-off generator (development tool, not part of the compiler): turns GHC's primop table
(compiler/GHC/Builtin/primops.txt.pp of the reference sources) into the GHC.Prim module that
the libraries import. Each primop becomes `foreign import prim "<symbol>" <name> :: <type>`,
which the runtime implements by name. Usage: gen_primops.py primops.txt.pp > GHC/Prim.hs
"""
import re
import sys

SKIP_TYPES = set()

# (name, primitive type, size in bytes, kind) of the element types of byte arrays.
ELEMENTS = [
    ("Char", "Char#", 1, "char8"),
    ("WideChar", "Char#", 4, "char32"),
    ("Int", "Int#", 8, "int"),
    ("Word", "Word#", 8, "word"),
    ("Int8", "Int8#", 1, "int"),
    ("Int16", "Int16#", 2, "int"),
    ("Int32", "Int32#", 4, "int"),
    ("Int64", "Int64#", 8, "int"),
    ("Word8", "Word8#", 1, "word"),
    ("Word16", "Word16#", 2, "word"),
    ("Word32", "Word32#", 4, "word"),
    ("Word64", "Word64#", 8, "word"),
    ("Double", "Double#", 8, "double"),
    ("Float", "Float#", 4, "float"),
    ("Addr", "Addr#", 8, "addr"),
    ("StablePtr", "StablePtr# a", 8, "addr"),
]


def parse(text):
    lines = text.split("\n")
    prims, types = [], []
    i = 0
    while i < len(lines):
        line = lines[i]
        m = re.match(r'^primtype\s+(\S+?)(?:\s+(.*))?$', line)
        if m:
            head = m.group(1)
            params = (m.group(2) or "").split()
            types.append((head, params))
            i += 1
            continue
        m = re.match(r'^primop\s+(\w+)\s+"([^"]+)"', line)
        if m:
            sym = m.group(2)
            rest = line[m.end():].strip()
            # The kind word (GenPrimOp, Compare, ...) may follow the symbol on this line or the next.
            rest = rest
            body = [rest] if rest and "->" in rest else []
            i += 1
            while i < len(lines):
                t = lines[i].strip()
                # The type ends at a blank line, a description block, `with`, or a directive.
                # A line starting with `->` continues the type (a multi-line signature).
                continues = t.startswith("->") or (t and t[0] not in "{}#-")
                if not t or not continues or t.startswith("with") or re.match(r'^(primop|primtype|pseudoop)\b', t):
                    break
                body.append(t)
                i += 1
            ty = " ".join(body).strip()
            # The kind word (GenPrimOp, Compare, ...) precedes the type: drop it.
            toks = ty.split(None, 1)
            if len(toks) == 2 and re.match(r'^[A-Z][A-Za-z]*$', toks[0]) and "->" in toks[1]:
                ty = toks[1]
            if ty.startswith("->") or "{" in ty or "}" in ty:
                ty = ""
            # SIMD primops use VECTOR/SCALAR/VECTUPLE, which GHC expands itself; not supported.
            if ty and not re.search(r'\b(VECTOR|SCALAR|VECTUPLE)\b', ty):
                prims.append((sym, ty))
            continue
        i += 1
    return types, prims


def main():
    text = open(sys.argv[1], encoding="utf-8").read()
    types, prims = parse(text)
    out = []
    out.append("{-# LANGUAGE MagicHash, UnboxedTuples, NoImplicitPrelude, PolyKinds, KindSignatures #-}")
    out.append("-- Generated from GHC's primops.txt.pp (GHC 9.12.2). Do not edit by hand.")
    out.append("module GHC.Prim where")
    out.append("")
    seen = set()
    for head, params in types:
        name = head
        if name in SKIP_TYPES or name in seen:
            continue
        seen.add(name)
        out.append(f"data {name} " + " ".join(params))
    # Kinds that GHC wires in rather than declaring (GHC.Builtin.Types): the kind of
    # unlifted types and the kind of constraints, over the representation argument.
    # Pseudo-operations and the Addr# family, which GHC defines outside the table.
    out.append('foreign import prim "realWorld#" realWorld# :: State# RealWorld')
    out.append('foreign import prim "nullAddr#" nullAddr# :: Addr#')
    out.append('foreign import prim "proxy#" proxy# :: Proxy# a')
    out.append('foreign import prim "seq" seq :: a -> b -> b')
    # Coercions are the identity at run time; the Coercible constraint is not checked.
    out.append('foreign import prim "unsafeCoerce" coerce :: a -> b')
    # Element access on byte arrays and on Addr#, for each element type (GHC generates these
    # from a template; the sizes and types follow the element's primitive type).
    for name, prim, _size, _kind in ELEMENTS:
        out.append(f'foreign import prim "index{name}Array#" index{name}Array# :: ByteArray# -> Int# -> {prim}')
        out.append(f'foreign import prim "read{name}Array#" read{name}Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, {prim} #)')
        out.append(f'foreign import prim "write{name}Array#" write{name}Array# :: MutableByteArray# s -> Int# -> {prim} -> State# s -> State# s')
        out.append(f'foreign import prim "index{name}OffAddr#" index{name}OffAddr# :: Addr# -> Int# -> {prim}')
        out.append(f'foreign import prim "read{name}OffAddr#" read{name}OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, {prim} #)')
        out.append(f'foreign import prim "write{name}OffAddr#" write{name}OffAddr# :: Addr# -> Int# -> {prim} -> State# s -> State# s')
    out.append("data TYPE r")
    out.append("data CONSTRAINT r")
    out.append("")
    done = set()
    for sym, ty in prims:
        if sym in done:
            continue  # the table repeats some names for platform variants; keep the first
        done.add(sym)
        name = sym
        is_op = not re.match(r'^[a-z_]', sym)
        shown = f"({name})" if is_op else name
        out.append(f'foreign import prim "{sym}" {shown} :: {ty}')
    print("\n".join(out))


if __name__ == "__main__":
    main()

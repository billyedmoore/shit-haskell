{-# LANGUAGE NoImplicitPrelude #-}
-- The Prelude: exports the standard names defined in Base.
module Prelude (
    Bool(False, True), (&&), (||), not, otherwise,
    Maybe(Nothing, Just), maybe,
    Either(Left, Right), either,
    Ordering(LT, EQ, GT), Char, String,
    fst, snd, curry, uncurry,
    Eq((==), (/=)),
    Ord(compare, (<), (<=), (>=), (>), max, min),
    Enum(succ, pred, toEnum, fromEnum, enumFrom, enumFromThen, enumFromTo, enumFromThenTo),
    Bounded(minBound, maxBound),
    Int, Integer, Float, Double, Rational,
    Num((+), (-), (*), negate, abs, signum, fromInteger),
    Real(toRational),
    Integral(quot, rem, div, mod, quotRem, divMod, toInteger),
    Fractional((/), recip, fromRational),
    Floating(pi, exp, log, sqrt, (**), sin, cos, tan),
    RealFrac(properFraction, truncate, round, ceiling, floor),
    subtract, even, odd, gcd, lcm, (^), (^^), fromIntegral, realToFrac,
    Semigroup((<>)), Monoid(mempty, mappend, mconcat),
    Functor(fmap, (<$)), (<$>),
    Applicative(pure, (<*>), (*>), (<*)),
    Monad((>>=), (>>), return), MonadFail(fail),
    Foldable(foldr, foldl, foldl', foldr1, foldl1, null, length, elem, maximum, minimum, sum, product),
    Traversable(traverse, sequenceA, mapM, sequence),
    mapM_, sequence_, (=<<),
    Show(showsPrec, show, showList), shows, ShowS, showChar, showString, showParen,
    Read(readsPrec, readList), ReadS, reads, read,

    IO, putChar, putStr, putStrLn, print, getChar, getLine, getContents, interact,
    FilePath, readFile, readLn, ioError, userError, IOError,
    id, const, (.), flip, ($), ($!), seq, error, undefined, until, asTypeOf,
    map, (++), filter, head, last, tail, init, (!!), reverse,
    and, or, any, all, concat, concatMap,
    scanl, scanl1, scanr, scanr1, iterate, repeat, replicate, cycle,
    take, drop, splitAt, takeWhile, dropWhile, span, break,
    notElem, lookup, zip, zip3, zipWith, zipWith3, unzip, unzip3,
    lines, words, unlines, unwords
  ) where

import Base

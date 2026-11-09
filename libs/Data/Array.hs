module Data.Array (Array, Ix(..), array, listArray, (!), bounds, elems, indices, assocs, accumArray, (//), accum, ixmap) where

import Base

infixl 9 !, //

-- Arrays are immutable vectors of values, indexed through the Ix class.
data Array i e = MkArray (i, i) Int (Vec e)

data Vec e

foreign import prim "vecFromList" primVecFromList :: Int -> [e] -> Vec e
foreign import prim "vecIndex" primVecIndex :: Vec e -> Int -> e
foreign import prim "vecToList" primVecToList :: Vec e -> [e]
foreign import prim "vecUpdate" primVecUpdate :: Vec e -> [(Int, e)] -> Vec e

class Ord a => Ix a where
  range :: (a, a) -> [a]
  index :: (a, a) -> a -> Int
  inRange :: (a, a) -> a -> Bool
  rangeSize :: (a, a) -> Int
  rangeSize b@(_, h) = if inRange b h then index b h + 1 else 0

instance Ix Int where
  range (l, h) = [l .. h]
  index (l, h) i = if i >= l && i <= h then i - l else error "Ix{Int}.index: Index out of range."
  inRange (l, h) i = i >= l && i <= h

instance Ix Integer where
  range (l, h) = [l .. h]
  index (l, h) i = if i >= l && i <= h then fromInteger (i - l) else error "Ix{Integer}.index: Index out of range."
  inRange (l, h) i = i >= l && i <= h

instance Ix Char where
  range (l, h) = [l .. h]
  index (l, h) i = if i >= l && i <= h then ord i - ord l else error "Ix{Char}.index: Index out of range."
  inRange (l, h) i = i >= l && i <= h

instance Ix Bool where
  range (l, h) = [l .. h]
  index (l, h) i = if i >= l && i <= h then fromEnum i - fromEnum l else error "Ix{Bool}.index: Index out of range."
  inRange (l, h) i = i >= l && i <= h

instance (Ix a, Ix b) => Ix (a, b) where
  range ((l1, l2), (h1, h2)) = [(a, b) | a <- range (l1, h1), b <- range (l2, h2)]
  index ((l1, l2), (h1, h2)) (a, b) = index (l1, h1) a * rangeSize (l2, h2) + index (l2, h2) b
  inRange ((l1, l2), (h1, h2)) (a, b) = inRange (l1, h1) a && inRange (l2, h2) b

array :: Ix i => (i, i) -> [(i, e)] -> Array i e
array b ies =
  let n = rangeSize b
      slots = [(index b i, e) | (i, e) <- ies]
  in MkArray b n (primVecUpdate (primVecFromList n (replicate n arrEleBottom)) slots)

arrEleBottom :: a
arrEleBottom = error "(Array.!): undefined array element"

listArray :: Ix i => (i, i) -> [e] -> Array i e
listArray b es =
  let n = rangeSize b
  in MkArray b n (primVecFromList n (take n es))

(!) :: Ix i => Array i e -> i -> e
(!) (MkArray b _ v) i = primVecIndex v (index b i)

bounds :: Array i e -> (i, i)
bounds (MkArray b _ _) = b

elems :: Array i e -> [e]
elems (MkArray _ _ v) = primVecToList v

indices :: Ix i => Array i e -> [i]
indices (MkArray b _ _) = range b

assocs :: Ix i => Array i e -> [(i, e)]
assocs a = zip (indices a) (elems a)

accumArray :: Ix i => (e -> a -> e) -> e -> (i, i) -> [(i, a)] -> Array i e
accumArray f z b ies =
  let n = rangeSize b
      step vec (i, a) = primVecUpdate vec [(index b i, f (primVecIndex vec (index b i)) a)]
  in MkArray b n (foldl step (primVecFromList n (replicate n z)) ies)

(//) :: Ix i => Array i e -> [(i, e)] -> Array i e
(//) (MkArray b n v) ies = MkArray b n (primVecUpdate v [(index b i, e) | (i, e) <- ies])

accum :: Ix i => (e -> a -> e) -> Array i e -> [(i, a)] -> Array i e
accum f arr ies = foldl (\a (i, x) -> a // [(i, f (a ! i) x)]) arr ies

ixmap :: (Ix i, Ix j) => (i, i) -> (i -> j) -> Array j e -> Array i e
ixmap b f a = listArray b [a ! f i | i <- range b]

instance (Show i, Ix i, Show e) => Show (Array i e) where
  showsPrec p a = showParen (p > 9) (showString "array " . showsPrec 11 (bounds a) . showChar ' ' . showsPrec 11 (assocs a))

instance (Eq i, Eq e) => Eq (Array i e) where
  MkArray b1 _ v1 == MkArray b2 _ v2 = b1 == b2 && primVecToList v1 == primVecToList v2

instance Functor (Array i) where
  fmap f (MkArray b n v) = MkArray b n (primVecFromList n (map f (primVecToList v)))

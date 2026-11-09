{-# LANGUAGE MagicHash #-}
-- Unboxed types share the boxed representation in this implementation: Int# is Int, and the
-- boxing constructors (I#, D#, C#) are identities (see the checker).
module GHC.Exts (
    Int#, Double#, Char#, Int, Double, Char, IntBox(I#), DoubleBox(D#), CharBox(C#),
    (+#), (-#), (*#), (==#), (/=#), (<#), (<=#), (>#), (>=#),
    isTrue#, quotInt#, remInt#, negateInt#, int2Double#, (+##), (-##), (*##), (/##),
    sortWith, groupWith, the, IsString(..), IsList(..), lazy, inline, oneShot
  ) where

import Base
import Data.List (sortBy, groupBy)

type Int# = Int
type Double# = Double
type Char# = Char

-- The boxing constructors. The checker treats them as identities (see `is_boxing_con`).
data IntBox = I# Int#
data DoubleBox = D# Double#
data CharBox = C# Char#

infixl 6 +#, -#
infixl 7 *#
infix 4 ==#, /=#, <#, <=#, >#, >=#

(+#), (-#), (*#), quotInt#, remInt# :: Int# -> Int# -> Int#
(+#) = (+)
(-#) = (-)
(*#) = (*)
quotInt# = quot
remInt# = rem

negateInt# :: Int# -> Int#
negateInt# = negate

(==#), (/=#), (<#), (<=#), (>#), (>=#) :: Int# -> Int# -> Int#
a ==# b = if a == b then 1 else 0
a /=# b = if a /= b then 1 else 0
a <# b = if a < b then 1 else 0
a <=# b = if a <= b then 1 else 0
a ># b = if a > b then 1 else 0
a >=# b = if a >= b then 1 else 0

isTrue# :: Int# -> Bool
isTrue# x = x /= 0

int2Double# :: Int# -> Double#
int2Double# = fromIntegral

(+##), (-##), (*##), (/##) :: Double# -> Double# -> Double#
(+##) = (+)
(-##) = (-)
(*##) = (*)
(/##) = (/)

sortWith :: Ord b => (a -> b) -> [a] -> [a]
sortWith f = sortBy (\x y -> compare (f x) (f y))

groupWith :: Ord b => (a -> b) -> [a] -> [[a]]
groupWith f xs = groupBy (\x y -> f x == f y) (sortWith f xs)

the :: Eq a => [a] -> a
the (x : xs) = if all (== x) xs then x else error "GHC.Exts.the: non-identical list"
the [] = error "GHC.Exts.the: empty list"

class IsString a where
  fromString :: String -> a

instance IsString [c] where
  fromString s = [] ++ [] `seq` (unsafeCoerceStr s)

unsafeCoerceStr :: String -> [c]
unsafeCoerceStr s = unsafeCoerceList s

foreign import prim "unsafeCoerce" unsafeCoerceList :: String -> [c]

class IsList l where
  fromListN :: Int -> [a] -> l

lazy :: a -> a
lazy x = x

inline :: a -> a
inline x = x

oneShot :: (a -> b) -> a -> b
oneShot f = f

{-# LANGUAGE MagicHash #-}
-- GHC.Exts: the primitive types and operations come from GHC.Prim and GHC.Types, as in GHC.
module GHC.Exts (
    module GHC.Prim, module GHC.Types,
    sortWith, groupWith, the, IsString(..), IsList(..), lazy, inline, oneShot
  ) where

import Base
import GHC.Prim
import GHC.Types
import Data.List (sortBy, groupBy)

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

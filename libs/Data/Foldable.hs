module Data.Foldable (
    Foldable(..), foldrM, foldlM, traverse_, for_, sequenceA_, asum, mapM_, forM_, sequence_,
    and, or, any, all, concat, concatMap, notElem, find, maximumBy, minimumBy
  ) where

import Base
import Control.Applicative (asum)
import Data.List (find, maximumBy, minimumBy)

foldrM :: (Foldable t, Monad m) => (a -> b -> m b) -> b -> t a -> m b
foldrM f z0 xs = foldl (\k x z -> f x z >>= k) return xs z0

foldlM :: (Foldable t, Monad m) => (b -> a -> m b) -> b -> t a -> m b
foldlM f z0 xs = foldr (\x k z -> f z x >>= k) return xs z0

traverse_ :: (Foldable t, Applicative f) => (a -> f b) -> t a -> f ()
traverse_ f xs = foldr (\x k -> f x *> k) (pure ()) xs

for_ :: (Foldable t, Applicative f) => t a -> (a -> f b) -> f ()
for_ xs f = traverse_ f xs

sequenceA_ :: (Foldable t, Applicative f) => t (f a) -> f ()
sequenceA_ xs = foldr (*>) (pure ()) xs

forM_ :: (Foldable t, Monad m) => t a -> (a -> m b) -> m ()
forM_ xs f = mapM_ f xs

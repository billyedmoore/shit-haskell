module Data.Traversable (Traversable(..), for, forM) where

import Base

for :: (Traversable t, Applicative f) => t a -> (a -> f b) -> f (t b)
for xs f = traverse f xs

forM :: (Traversable t, Monad m) => t a -> (a -> m b) -> m (t b)
forM xs f = mapM f xs

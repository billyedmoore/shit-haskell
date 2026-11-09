module Data.Ord (Ord(..), Ordering(..), comparing, Down(..), clamp) where

import Base

newtype Down a = Down a

instance Eq a => Eq (Down a) where
  Down x == Down y = x == y

instance Ord a => Ord (Down a) where
  compare (Down x) (Down y) = compare y x

instance Show a => Show (Down a) where
  showsPrec d (Down x) = showParen (d > 10) (showString "Down " . showsPrec 11 x)

clamp :: Ord a => (a, a) -> a -> a
clamp (low, high) a = min high (max a low)

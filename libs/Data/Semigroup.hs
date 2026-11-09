module Data.Semigroup (Semigroup(..), Monoid(..), Min(..), Max(..), Sum(..), Product(..), Any(..), All(..)) where

import Base
import Data.Monoid

newtype Min a = Min { getMin :: a }

newtype Max a = Max { getMax :: a }

instance Eq a => Eq (Min a) where
  Min a == Min b = a == b

instance Ord a => Ord (Min a) where
  compare (Min a) (Min b) = compare a b

instance Show a => Show (Min a) where
  showsPrec d (Min a) = showParen (d >= 11) (showString "Min {getMin = " . showsPrec 0 a . showChar '}')

instance Ord a => Semigroup (Min a) where
  Min a <> Min b = Min (min a b)

instance (Ord a, Bounded a) => Monoid (Min a) where
  mempty = Min maxBound

instance Eq a => Eq (Max a) where
  Max a == Max b = a == b

instance Ord a => Ord (Max a) where
  compare (Max a) (Max b) = compare a b

instance Show a => Show (Max a) where
  showsPrec d (Max a) = showParen (d >= 11) (showString "Max {getMax = " . showsPrec 0 a . showChar '}')

instance Ord a => Semigroup (Max a) where
  Max a <> Max b = Max (max a b)

instance (Ord a, Bounded a) => Monoid (Max a) where
  mempty = Max minBound

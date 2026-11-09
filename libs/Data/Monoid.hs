module Data.Monoid (Monoid(..), Semigroup(..), Sum(..), Product(..), Any(..), All(..), First(..), Last(..), (<>)) where

import Base

newtype Sum a = Sum { getSum :: a }
newtype Product a = Product { getProduct :: a }
newtype Any = Any { getAny :: Bool }
newtype All = All { getAll :: Bool }
newtype First a = First { getFirst :: Maybe a }
newtype Last a = Last { getLast :: Maybe a }

instance Num a => Semigroup (Sum a) where
  Sum a <> Sum b = Sum (a + b)
instance Num a => Monoid (Sum a) where
  mempty = Sum 0

instance Num a => Semigroup (Product a) where
  Product a <> Product b = Product (a * b)
instance Num a => Monoid (Product a) where
  mempty = Product 1

instance Semigroup Any where
  Any a <> Any b = Any (a || b)
instance Monoid Any where
  mempty = Any False

instance Semigroup All where
  All a <> All b = All (a && b)
instance Monoid All where
  mempty = All True

instance Semigroup (First a) where
  First Nothing <> b = b
  a <> _ = a
instance Monoid (First a) where
  mempty = First Nothing

instance Semigroup (Last a) where
  a <> Last Nothing = a
  _ <> b = b
instance Monoid (Last a) where
  mempty = Last Nothing

instance Show a => Show (Sum a) where
  showsPrec d (Sum x) = showParen (d > 10) (showString "Sum {getSum = " . showsPrec 0 x . showChar '}')
instance Show a => Show (Product a) where
  showsPrec d (Product x) = showParen (d > 10) (showString "Product {getProduct = " . showsPrec 0 x . showChar '}')

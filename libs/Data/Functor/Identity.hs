module Data.Functor.Identity (Identity(..)) where

import Base

newtype Identity a = Identity { runIdentity :: a }

instance Show a => Show (Identity a) where
  showsPrec d (Identity x) = showParen (d > 10) (showString "Identity " . showsPrec 11 x)

instance Functor Identity where
  fmap f (Identity x) = Identity (f x)

instance Applicative Identity where
  pure x = Identity x
  Identity f <*> Identity x = Identity (f x)

instance Monad Identity where
  Identity x >>= k = k x

instance MonadFix Identity where
  mfix f = Identity (let a = runIdentity (f a) in a)

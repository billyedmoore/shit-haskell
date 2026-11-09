{-# LANGUAGE NoImplicitPrelude #-}
module Data.List.NonEmpty (NonEmpty(..), toList, fromList, head, tail, map, nonEmpty, length) where

import Base hiding (head, tail, map, length, toList)
import qualified Base as B

infixr 5 :|

data NonEmpty a = a :| [a]

instance Eq a => Eq (NonEmpty a) where
  (a :| as) == (b :| bs) = a == b && as == bs

instance Show a => Show (NonEmpty a) where
  showsPrec d (a :| as) = showParen (d > 5) (showsPrec 6 a . showString " :| " . showsPrec 6 as)

instance Functor NonEmpty where
  fmap f (a :| as) = f a :| B.map f as

toList :: NonEmpty a -> [a]
toList (a :| as) = a : as

fromList :: [a] -> NonEmpty a
fromList (a : as) = a :| as
fromList [] = error "NonEmpty.fromList: empty list"

nonEmpty :: [a] -> Maybe (NonEmpty a)
nonEmpty [] = Nothing
nonEmpty (a : as) = Just (a :| as)

head :: NonEmpty a -> a
head (a :| _) = a

tail :: NonEmpty a -> [a]
tail (_ :| as) = as

map :: (a -> b) -> NonEmpty a -> NonEmpty b
map f (a :| as) = f a :| B.map f as

length :: NonEmpty a -> Int
length (_ :| as) = 1 + B.length as

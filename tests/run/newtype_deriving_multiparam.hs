{-# LANGUAGE GeneralizedNewtypeDeriving, StandaloneDeriving, MultiParamTypeClasses, FunctionalDependencies, FlexibleInstances, UndecidableInstances #-}
-- Newtype deriving of a multi-parameter class at its last parameter, the others given
-- (witherable's deriving instance FilterableWithIndex i f => FilterableWithIndex i (IdentityT f)).
module Main where

class Functor t => Indexed i t | t -> i where
  imap :: (i -> a -> b) -> t a -> t b
  indices :: t a -> [i]

instance Indexed Int [] where
  imap f = zipWith f [0 ..]
  indices xs = [0 .. length xs - 1]

class Convert a b where
  convert :: a -> b

instance Convert String Int where
  convert = length

newtype Wrap f a = Wrap (f a) deriving (Show, Functor)
deriving instance Indexed i f => Indexed i (Wrap f)

newtype Name = Name Int deriving (Show, Convert String)

main :: IO ()
main = do
  print (imap (\i x -> i * 10 + x) (Wrap [1, 2, 3 :: Int]))
  print (indices (Wrap "abc"))
  print (convert "hello" :: Name)

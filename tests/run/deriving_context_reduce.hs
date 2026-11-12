{-# LANGUAGE DatatypeContexts #-}
{-# OPTIONS_GHC -Wno-deprecated-flags #-}
-- Derived instance contexts reduce the fields' constraints through instances (alex's RangedSet).
module Main where

class Ord a => Discrete a where
  after :: a -> a

instance Discrete Int where
  after = (+ 1)

data Bound a = Below a | Above a

instance Discrete a => Eq (Bound a) where
  x == y = key x == key y

instance Discrete a => Ord (Bound a) where
  compare x y = compare (key x) (key y)

instance Show a => Show (Bound a) where
  show (Below a) = "<" ++ show a
  show (Above a) = ">" ++ show a

key :: Discrete a => Bound a -> a
key (Below a) = a
key (Above a) = after a

data Range v = Range (Bound v) (Bound v) deriving (Eq, Ord, Show)

newtype Discrete v => RSet v = RSet [Range v] deriving (Eq, Show, Ord)

data Tree a = Leaf | Node (Tree a) (Maybe a) [Tree a] deriving (Eq, Show)

main :: IO ()
main = do
  let r1 = Range (Below 1) (Above (3 :: Int))
      r2 = Range (Below 1) (Below (4 :: Int))
  print (r1 == r2, compare r1 r2)
  print (RSet [r1] == RSet [r2], RSet [r1] < RSet [r1, r2])
  print (RSet [r1])
  print (Node Leaf (Just 'x') [Leaf] == Node Leaf (Just 'x') [Leaf], Node Leaf Nothing [] :: Tree Int)

{-# LANGUAGE ConstraintKinds #-}
{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE FunctionalDependencies #-}
{-# LANGUAGE GADTs #-}
{-# LANGUAGE MultiParamTypeClasses #-}
{-# LANGUAGE QuantifiedConstraints #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE TypeOperators #-}
{-# LANGUAGE UndecidableInstances #-}
{-# LANGUAGE UndecidableSuperClasses #-}
module Main where

import Data.Kind (Constraint)

data Dict c where
  Dict :: c => Dict c

class HasDict c e | e -> c where
  evidence :: e -> Dict c

instance HasDict c (Dict c) where
  evidence = id

withDict :: HasDict c e => e -> (c => r) -> r
withDict d r = case evidence d of
  Dict -> r

class (p => q) => p |- q
instance (p => q) => p |- q

useImpl :: (Ord a, Ord a |- Eq a) => a -> a -> Bool
useImpl x y = x == y

weaken :: (p => q) => Dict (p |- q)
weaken = Dict

applyDict :: (p |- q) => Dict p -> Dict q
applyDict Dict = Dict

main :: IO ()
main = do
  print (useImpl (3 :: Int) 3)
  print (withDict (Dict :: Dict (Eq Int)) ((2 :: Int) == 2))
  case (weaken :: Dict (Ord Int |- Eq Int)) of
    Dict -> case applyDict (Dict :: Dict (Ord Int)) :: Dict (Eq Int) of
      Dict -> print ((1 :: Int) == 1)

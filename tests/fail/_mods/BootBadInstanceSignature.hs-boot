{-# LANGUAGE InstanceSigs #-}
module BootBadInstanceSignature where
class Check a where
  check :: a -> a
instance Check Int where
  check :: Int -> Int

{-# LANGUAGE MagicHash, PolyKinds, DataKinds, KindSignatures, TypeFamilies, FlexibleInstances, UnliftedNewtypes #-}
-- Instances told apart only by the kind of their variable, with an equality context that fixes
-- the type (ghc-heap's `instance Int# ~ a => HasHeapRep (a :: TYPE 'IntRep)`).
module Main where

import GHC.Exts

class Describe (a :: TYPE rep) where
  describe :: a -> String

instance Int# ~ a => Describe (a :: TYPE 'IntRep) where
  describe x = "Int# " ++ show (I# x)

instance Double# ~ a => Describe (a :: TYPE 'DoubleRep) where
  describe x = "Double# " ++ show (D# x)

instance Describe (a :: TYPE ('BoxedRep 'Lifted)) where
  describe _ = "boxed"

main :: IO ()
main = do
  putStrLn (describe 3#)
  putStrLn (describe 2.5##)
  putStrLn (describe 'c')

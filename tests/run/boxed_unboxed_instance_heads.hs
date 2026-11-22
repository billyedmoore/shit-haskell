{-# LANGUAGE KindSignatures, PolyKinds, UnboxedTuples #-}
module DistinctInstanceHeads where

class C (a :: k)
instance C ()
instance C (# #)

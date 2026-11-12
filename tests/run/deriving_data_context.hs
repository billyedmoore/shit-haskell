{-# LANGUAGE DeriveDataTypeable #-}
-- A derived Data instance needs what its fields' Data instances need (`Data (Set a)` needs
-- `Ord a`), as Cabal-syntax's NonEmptySet derives it; and `main` without a signature is IO.
module Main where

import Data.Data (Data, dataTypeOf, toConstr, gmapQ, Proxy (..), typeRep)
import qualified Data.Set as Set

newtype NonEmptySet a = NES (Set.Set a)
  deriving (Eq, Ord, Data, Read, Show)

data Tagged t a = Tagged a deriving (Data, Show)

main = do
  let s = NES (Set.fromList "hello")
  print (s, toConstr s, dataTypeOf s)
  print (read "NES (fromList [3,1,2])" == NES (Set.fromList [1, 2, 3 :: Int]))
  print (toConstr (Tagged 'x' :: Tagged Maybe Char), gmapQ (const ()) (Tagged True :: Tagged Int Bool))
  return ()

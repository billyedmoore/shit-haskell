{-# LANGUAGE TemplateHaskell, DeriveLift #-}
-- Lifting values into splices: bytestring's Lift instances (a BytesPrimL literal) and derived
-- Lift instances (bytestring's own test suite).
module Main where

import qualified Data.ByteString as BS
import qualified Data.ByteString.Lazy as LBS
import qualified Data.ByteString.Short as SBS
import qualified Language.Haskell.TH.Syntax as TH
import LiftTypes

main :: IO ()
main = do
  print ($(TH.lift (BS.pack [102, 111, 0, 200, 255])) :: BS.ByteString)
  print ($$(TH.liftTyped (LBS.pack [0, 1, 2, 3])) :: LBS.ByteString)
  print ($(TH.lift (SBS.pack [104, 105])) :: SBS.ShortByteString)
  print $(TH.lift (Leaf 3 :| Node (Leaf 1) "x" (Leaf (-2))))
  print $$(TH.liftTyped (Just (Pair 'q' [1.5, 2])))

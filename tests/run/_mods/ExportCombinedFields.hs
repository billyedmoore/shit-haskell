{-# LANGUAGE DuplicateRecordFields #-}
module ExportCombinedFields (A(..), B(..)) where

data A = A { label :: Int }
data B = B { label :: Bool }

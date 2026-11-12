{-# LANGUAGE PatternSynonyms #-}
module Rec (R (R, name, ..), pattern Origin) where

data R = R { name :: String, x :: Int, y :: Int } deriving Show

pattern Origin :: R
pattern Origin = R "origin" 0 0

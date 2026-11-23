{-# LANGUAGE PatternSynonyms #-}
module Main (main, T(Here, Other)) where

data T = Here

pattern Other :: T
pattern Other = Here

main :: IO ()
main = case Here of Other -> print True

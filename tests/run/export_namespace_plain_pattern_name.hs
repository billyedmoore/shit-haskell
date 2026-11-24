{-# LANGUAGE NoPatternSynonyms #-}
module Main (main, pattern) where

pattern :: Int
pattern = 4

main :: IO ()
main = print pattern

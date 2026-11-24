{-# LANGUAGE PatternSynonyms #-}
module Main (main, T(value)) where

data T = Here

value :: Int
value = 3

main :: IO ()
main = pure ()

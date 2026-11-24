{-# LANGUAGE ExplicitNamespaces, NoPatternSynonyms #-}
module Main (main, T(.., Here)) where

data T = Here

main :: IO ()
main = pure ()

{-# LANGUAGE PatternSynonyms #-}
module Main (main, T(field)) where

data T = Here
data U = There { field :: Int }

main :: IO ()
main = pure ()

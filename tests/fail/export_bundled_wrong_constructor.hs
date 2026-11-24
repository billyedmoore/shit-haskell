{-# LANGUAGE PatternSynonyms #-}
module Main (main, T(There)) where

data T = Here
data U = There

main :: IO ()
main = pure ()

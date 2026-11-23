{-# LANGUAGE DuplicateRecordFields #-}
module Main (main, A(..), B(..)) where

data A = A { label :: Int }
data B = B { label :: Bool }

main :: IO ()
main = pure ()

{-# LANGUAGE TypeFamilies #-}
module Main where

type family Result a where
  Main.Result Int = Bool

answer :: Result Int
answer = True

main :: IO ()
main = print answer

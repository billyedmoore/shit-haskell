module Main (main) where

import Toy.Greet (greet)

main :: IO ()
main = putStrLn (greet "cabal")

-- A module named GHC.* with its own foldr and (++): its list comprehensions use the list ones.
module Main where

import GHC.Data.Stream (prefixes, takeS)

main :: IO ()
main = print (takeS 6 (prefixes "ab"))

{-# LANGUAGE ViewPatterns #-}
-- A view pattern as a record pattern's field (ghc-lib-parser's
-- `VI{ vi_pos = as_newtype -> Just y }`).
module Main where

import Data.Maybe (listToMaybe)

data Info = Info { pos :: [Int], name :: String }

firstPos :: Info -> String
firstPos Info { pos = listToMaybe -> Just p, name = n } = n ++ " at " ++ show p
firstPos Info { name = (length -> l) } = "nothing in " ++ show l ++ " chars"

main :: IO ()
main = mapM_ (putStrLn . firstPos) [Info [3, 4] "a", Info [] "bcd"]

{-# LANGUAGE QuasiQuotes #-}
-- Quasi-quotations: the quoter (imported) turns the text into an expression at compile time.
module Main where

import QQ
main :: IO ()
main = do
  putStrLn [str|hello "world" \ back|]
  putStrLn [str|two
lines|]
  print [shout|abc|]
  print [x | x <- [1 :: Int, 2]]

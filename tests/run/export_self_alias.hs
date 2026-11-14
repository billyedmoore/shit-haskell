{-# LANGUAGE NoImplicitPrelude #-}
-- An export of a module's own name that is also an import alias exports both.
module Main where

import Lib.Prelude

main :: IO ()
main = do
  print (twice (head [21 :: Int, 0]))
  print (fromMaybe (0 :: Int) (Just 3))
  putStrLn (show (length "abc"))

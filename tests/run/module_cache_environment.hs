{-# LANGUAGE TypeFamilies #-}
-- Imported modules contribute every part of their environment, including equations for
-- another module's family, derived dictionaries, synonyms and record selectors.
module Main where

import CacheTypes
import CacheInstances

main :: IO ()
main = do
  let b = wrap (7 :: Int)
  putStrLn (describe b)
  print b
  print (unwrap b + getBox b)
  putStrLn (describe (wrap "text"))

{-# LANGUAGE TemplateHaskell #-}
-- Type and pattern quotations ([t| .. |], [p| .. |]) used to build splices.
module Main where

import QuoteTP

main :: IO ()
main = do
  print $(annotated 7)
  print $(typedNothing)
  print ($(swapper) (1 :: Int, "two"))
  mapM_ (putStrLn . $(pairSum)) [(0, Nothing), (2, Just 3), (5, Nothing :: Maybe Int)]

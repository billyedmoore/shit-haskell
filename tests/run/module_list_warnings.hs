{-# OPTIONS_GHC -Wmissing-export-lists -Wmissing-import-lists #-}

module Main where

import Data.Maybe

main :: IO ()
main = print (isJust (Just (3 :: Int)))

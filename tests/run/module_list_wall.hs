{-# OPTIONS_GHC -Wall -Wextra -Werror #-}

module Main where

import Data.Maybe

main :: IO ()
main = print (isJust (Just (3 :: Int)))

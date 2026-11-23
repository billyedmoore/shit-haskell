{-# OPTIONS_GHC -Werror=duplicate-exports #-}
module Main (main, module Data.Maybe, Maybe(Just)) where

import Data.Maybe

main :: IO ()
main = pure ()

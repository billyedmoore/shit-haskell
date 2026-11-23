{-# OPTIONS_GHC -Werror=duplicate-exports #-}
module Main (main, module One, module Two) where

import Data.Maybe as One
import Data.Maybe as Two

main :: IO ()
main = pure ()

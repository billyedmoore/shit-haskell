{-# OPTIONS_GHC -Werror=missing-export-lists -Werror=missing-import-lists #-}

module Main (main) where

import qualified Data.Maybe as Maybe
import Data.List (sort)

main :: IO ()
main = print (Maybe.isJust (Just (sort [2 :: Int, 1])))

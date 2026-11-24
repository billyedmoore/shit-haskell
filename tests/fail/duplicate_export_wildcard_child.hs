{-# LANGUAGE PatternSynonyms #-}
{-# OPTIONS_GHC -Werror=duplicate-exports #-}
module Main (main, T(.., field)) where

data T = T { field :: Int }

main :: IO ()
main = pure ()

{-# OPTIONS_GHC -Werror=duplicate-exports #-}
module Main (main, value, Main.value) where

value :: Int
value = 2

main :: IO ()
main = pure ()

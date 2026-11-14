{-# OPTIONS_GHC -cpp -Wno-unused-imports #-}
-- CPP turned on by `-cpp` in an OPTIONS_GHC pragma (GHC's OccurAnal).
module Main where

#define GREETING "hello"

main :: IO ()
#ifdef DEBUG
main = putStrLn "debug"
#else
main = putStrLn GREETING
#endif

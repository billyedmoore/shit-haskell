{-# LANGUAGE ViewPatterns #-}
-- View patterns as list-pattern elements (ShellCheck's `[asStr -> Just "test", lhs, ..]`).
module Main where

import Text.Read (readMaybe)

classify :: [String] -> String
classify [readMaybe -> Just n, op, (readMaybe -> Just m)] = op ++ " " ++ show (n + m :: Int)
classify [length -> 1, rest] = "single then " ++ rest
classify (map length -> lens) = "lengths " ++ show lens

main :: IO ()
main = mapM_ (putStrLn . classify) [["1", "+", "2"], ["x", "y"], ["ab", "c", "def"]]

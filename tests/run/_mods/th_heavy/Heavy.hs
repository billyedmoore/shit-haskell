{-# LANGUAGE TemplateHaskell #-}
-- | A splice that computes a lot at compile time (more than the interpreter's allocation
-- budget, so it runs as a native program).
module Heavy (digitCount, bigList) where

import Language.Haskell.TH

digitCount :: Int -> Q Exp
digitCount n = litE (integerL (fromIntegral (length (concatMap show [1 .. n]))))

bigList :: Q Exp
bigList = listE [tupE [litE (stringL (show i)), litE (charL (toEnum (65 + i `mod` 26)))] | i <- [1 .. 300 :: Int]]

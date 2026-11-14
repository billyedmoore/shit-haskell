{-# LANGUAGE PatternSynonyms, UnboxedTuples, UnboxedSums #-}
-- A bidirectional pattern synonym used as a backticked operator, in a section (GHC's OrdList).
module Main where

type VMaybe a b = (# (# a, b #) | (# #) #)

pattern VJust :: a -> b -> VMaybe a b
pattern VJust a b = (# (# a, b #) | #)

pattern VNothing :: VMaybe a b
pattern VNothing = (# | (# #) #)
{-# COMPLETE VJust, VNothing #-}

pattern Pair :: a -> b -> (a, b)
pattern Pair x y = (x, y)

lastSplit :: [Int] -> VMaybe [Int] Int
lastSplit [] = VNothing
lastSplit xs = (`VJust` last xs) $ init xs

describe :: VMaybe [Int] Int -> String
describe (VJust as a) = show as ++ " then " ++ show a
describe VNothing = "empty"

main :: IO ()
main = do
  putStrLn (describe (lastSplit [1, 2, 3]))
  putStrLn (describe (lastSplit []))
  print (1 `Pair` 'x', (`Pair` "r") True, (2 `Pair`) 'y')

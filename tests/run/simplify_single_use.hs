{-# LANGUAGE BangPatterns #-}
-- Single-use local bindings move to their use, and cases on `otherwise` fold. A guarded
-- accumulating loop must run in constant space; laziness and sharing must be kept.
import Control.Exception
import Debug.Trace (trace)

loop :: Int -> Int -> Int
loop !acc i
  | i > 300000 = acc
  | even i, i `mod` 3 == 0 = loop (acc + 2) (i + 1)
  | otherwise = loop (acc + i `mod` 7) (i + 1)

-- A binding used once inside a branch that is not taken is never evaluated.
lazyBranch :: Bool -> Int
lazyBranch b = let x = error "forced" :: Int in if b then x else 1

-- A binding used once under a lambda stays shared between the calls.
shared :: Int -> [Int]
shared n = let x = trace "computed once" (n * 2) in map (\k -> k + x) [1, 2, 3]

-- The case binder of a folded case refers to the constructor.
binder :: Int -> String
binder n = case otherwise of
  b | b, n > 0 -> "positive " ++ show b
  _ -> "other"

-- Bindings used once in a sibling's right-hand side.
siblings :: Int -> Int
siblings n = let a = n + 1; b = a * 2; c = b - 3 in c + n

main :: IO ()
main = do
  print (loop 0 1)
  print (lazyBranch False)
  r <- try (evaluate (lazyBranch True))
  putStrLn (either (\(ErrorCall m) -> "caught " ++ m) show r)
  print (sum (shared 5))
  putStrLn (binder 3)
  putStrLn (binder 0)
  print (siblings 10)

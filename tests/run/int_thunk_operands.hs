-- Inline Int#/Char# primitives with thunk and indirection operands, including left-to-right
-- exception order and division edge cases.
import Control.Exception (SomeException, evaluate, try)
import Data.Char (chr, ord)
import Data.List (isInfixOf)

data Box = Box Int

catchMsg :: Int -> IO String
catchMsg x = do
  r <- try (evaluate x) :: IO (Either SomeException Int)
  pure (either classify (const "ok") r)
  where
    classify e
      | "first" `isInfixOf` msg = "first"
      | "second" `isInfixOf` msg = "second"
      | otherwise = "err"
      where
        msg = show e

main :: IO ()
main = do
  -- Lazy constructor fields (thunks, then updated INDs on second read).
  let bs = [Box (i * 3 + 1) | i <- [1 .. 20 :: Int]]
      sumBox [] = 0
      sumBox (Box x : xs) = x + sumBox xs
  print (sumBox bs, sumBox bs)
  -- Same thunk used as both operands (x * x) and chained arithmetic.
  let f :: Int -> Int
      f n = let t = n + 5 in (t * t) + (t - 2) `quot` 3 + (t `rem` 7)
  print (map f [-5 .. 5])
  -- Char# operations on thunks.
  let cs = [chr (65 + i) | i <- [0 .. 5 :: Int]]
  print (map ord cs, zipWith (<) cs (drop 1 cs))
  -- Left-to-right evaluation order and division by zero / minBound `quot` (-1).
  let e1 = error "first" :: Int
      e2 = error "second" :: Int
  putStrLn =<< catchMsg (e1 + e2)
  putStrLn =<< catchMsg (1 + e2)
  putStrLn =<< catchMsg (e1 `quot` 0)
  putStrLn =<< catchMsg ((10 + 20 :: Int) `quot` (5 - 5))
  putStrLn =<< catchMsg ((minBound :: Int) `quot` (-1))

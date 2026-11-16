{-# LANGUAGE MagicHash #-}
-- The simplifier's inlining, case-of-known and call-by-value rewrites, and the inline Int#
-- and literal-case fast paths: results, laziness and exceptions must not change.
import Control.Exception
import GHC.Exts

-- Lazy in y: a call with an undefined y must not evaluate it.
pick :: Int -> Int -> Int
pick x y = if x > 0 then y else 0

-- Strict in x only on the path that returns: an error alternative must not make x strict.
errs :: Bool -> Int -> Int
errs b x = case b of
  True -> error "first"
  False -> x + 1

-- Strict in both, recursively (arguments are evaluated before the call).
count :: Int -> Int -> Int
count acc n = if n == 0 then acc else count (acc + n `rem` 3) (n - 1)

classify :: Char -> String
classify 'a' = "letter a"
classify '\n' = "newline"
classify c = "other " ++ show c

dbl :: Double -> String
dbl 0.5 = "half"
dbl 1.0e10 = "big"
dbl _ = "other"

lit :: Int -> String
lit 0 = "zero"
lit (-1) = "minus one"
lit 9223372036854775807 = "max"
lit _ = "other"

data T = A Int | B deriving Show

known :: Int -> Int
known n = case A n of
  A k -> k * 2
  B -> 0

-- An action's later argument is evaluated only after the effect before it has run.
after :: IO () -> IO () -> IO ()
after a b = a >> b
{-# NOINLINE after #-}

main :: IO ()
main = do
  r0 <- try (after (putStrLn "effect first") (if length [()] > 5 then pure () else error "then the error"))
  putStrLn (either (\(ErrorCall m) -> m) show r0)
  print (pick 0 undefined)
  print (pick 1 7)
  r <- try (evaluate (errs True (error "second")))
  putStrLn (either (\(ErrorCall m) -> m) show r)
  print (errs False 4)
  print (count 0 100000)
  mapM_ (putStrLn . classify) "a\nz"
  mapM_ (putStrLn . dbl) [0.5, 1.0e10, 2]
  mapM_ (putStrLn . lit) [0, -1, maxBound, 3]
  print (known 21)
  print (minBound `rem` (-1 :: Int), I# (quotInt# 7# -2#), I# (remInt# -7# 2#))
  print (map (`div` 7) [-15, 15 :: Int], map (`mod` 7) [-15, 15 :: Int], map (`mod` (-7)) [-15, 15 :: Int])
  print (I# (uncheckedIShiftL# 3# 4#), I# (uncheckedIShiftRA# -64# 2#), I# (uncheckedIShiftRL# -1# 60#))
  print (I# (notI# 5#), I# (negateInt# 5#), maxBound + (1 :: Int), (maxBound :: Int) * 3)
  r2 <- try (evaluate (1 `rem` (0 :: Int)))
  putStrLn (either (\e -> "caught " ++ show (e :: ArithException)) show r2)
  print [x | x <- [1 .. 30 :: Int], x `mod` 3 == 0, odd x]

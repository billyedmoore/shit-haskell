{-# LANGUAGE MagicHash #-}
-- Workers whose result is a number return it raw: used raw by arithmetic, boxed where a value
-- is needed, inside mixed scrutinees, as a Char, through tail calls and join points.
import GHC.Exts

fib :: Int -> Int
fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)

-- Tail calls between workers that return raw.
collatz :: Int -> Int -> Int
collatz steps n
  | n == 1 = steps
  | even n = collatz (steps + 1) (n `quot` 2)
  | otherwise = up (steps + 1) n

up :: Int -> Int -> Int
up steps n = collatz steps (3 * n + 1)

-- A Char result.
rot :: Char -> Char
rot c = if c >= 'a' && c <= 'y' then succ c else if c == 'z' then 'a' else c

-- A result chosen through a join point (several guards share the fall-through).
grade :: Int -> Int
grade x
  | x > 90 = 4
  | x > 75, even x = 3
  | x > 75 = 2
  | otherwise = fib (x `rem` 10)

-- Mixed: one alternative answers a raw-returning call, the other a stored Int.
pick :: Bool -> Int -> [Int] -> Int
pick b n xs = case b of
  True -> fib n + 1
  False -> head xs

-- A tail-call cycle through a worker and a function without one stays a loop of jumps.
cycleF :: Int -> Int
cycleF 0 = 0
cycleF n = cycleG [n - 1]

cycleG :: [Int] -> Int
cycleG [m] = cycleF m
cycleG _ = 0

main :: IO ()
main = do
  print (cycleF 300000)
  print (fib 20, [fib k | k <- [0 .. 10]])
  print (map (collatz 0) [1, 7, 27, 97])
  putStrLn (map rot "hello, zebra")
  print (map grade [95, 80, 81, 40, 7])
  print (pick True 10 [], pick False 0 [42], sum (map (\k -> pick (odd k) k [k]) [1 .. 12]))
  let xs = map fib [15 .. 18]
  print (xs, fib 12 `seq` length xs)

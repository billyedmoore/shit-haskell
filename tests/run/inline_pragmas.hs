-- INLINE and NOINLINE pragmas in their forms (with phases, CONLIKE, on operators, in classes,
-- instances and local bindings): they change no result, and NOINLINE keeps sharing.
import Debug.Trace (trace)

infixl 6 <+>
(<+>) :: Int -> Int -> Int
x <+> y = x * 10 + y
{-# INLINE (<+>) #-}

step :: Int -> Int
step n = if even n then n `div` 2 else 3 * n + 1
{-# INLINE [1] step #-}

early :: Int -> Int
early = (+ 1)
{-# INLINE [~1] early #-}

big :: [Int] -> Int
big xs = sum [x * y | x <- xs, y <- xs, x /= y] + length xs + maximum xs - minimum xs
{-# INLINE CONLIKE big #-}

expensive :: Int -> Int
expensive n = trace "expensive" (n * n)
{-# NOINLINE expensive #-}

later :: Int -> Int
later n = n - 1
{-# NOINLINE [0] later #-}

class Shape a where
  area :: a -> Int
  {-# INLINE area #-}
  area _ = 0
  name :: a -> String

data Square = Square Int

instance Shape Square where
  area (Square s) = s * s
  {-# INLINE area #-}
  name _ = "square"
  {-# NOINLINE name #-}

collatz :: Int -> Int
collatz = go 0
  where
    go acc 1 = acc
    go acc n = go (acc + 1) (step n)
    {-# INLINE go #-}

main :: IO ()
main = do
  print (foldl (<+>) 0 [1, 2, 3], early 41, later 1)
  print (collatz 27, big [1 .. 10])
  let e = expensive 7
  print (e + e)
  print (area (Square 4), name (Square 4))

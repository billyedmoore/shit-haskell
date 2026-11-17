-- Functions returning functions or IO actions take another parameter only when the work before
-- the result is cheap: work worth sharing (a traced let, a call in a scrutinee) stays shared,
-- each trace prints as often as in GHC.
import Control.Monad (forM_, when)
import Data.IORef
import Debug.Trace (trace)

-- A let of real work before the lambda: not expanded, the work is shared.
scale :: Int -> Int -> Int
scale n = let t = trace "scale" (n * 2) in \y -> t + y
{-# NOINLINE scale #-}

-- A case of a call before the lambda: not expanded.
pick :: Int -> Int -> Int
pick n = case trace "pick" (n > 0) of
  True -> (+ n)
  False -> subtract n
{-# NOINLINE pick #-}

-- A case of a parameter: expanded; the action is the same each time it runs.
say :: Bool -> IO ()
say b = if b then putStrLn "yes" else putStrLn "no"
{-# NOINLINE say #-}

-- An IO loop over a list (expanded to take the state token).
count :: IORef Int -> [Int] -> IO ()
count _ [] = return ()
count r (x : xs) = do
  when (even x) (modifyIORef' r (+ x))
  count r xs

main :: IO ()
main = do
  print (map (scale 3) [1, 2, 3])
  print (map (pick 2) [1, 2], map (pick (-1)) [5])
  let act = say True
  act >> act >> say False
  r <- newIORef 0
  count r [1 .. 100]
  forM_ [1 .. 10 :: Int] $ \i -> modifyIORef r (* 1) >> when (i == 10) (putStrLn "ten")
  readIORef r >>= print

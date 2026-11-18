{-# LANGUAGE BangPatterns #-}
-- Collections at every kind of point in a body (allocation, calls in scrutinees, evaluation of
-- thunks, primitives) on some paths and not others, with many locals live at once, and
-- variables that are evaluated thunks (indirections) captured by closures.
import Data.IORef
import Data.List (foldl')

data T = L | N T !Int T

build :: Int -> T
build 0 = L
build n = let l = build (n `div` 2); r = build (n - 1 - n `div` 2) in N l n r

walk :: T -> Int -> Int
walk L acc = acc
walk (N l v r) acc
  | even v = walk l (walk r (acc + v))
  | otherwise = let xs = [v, v + 1, v + 2]; s = sum xs in walk r (walk l (acc + s))
{-# NOINLINE walk #-}

branchy :: Int -> [Int] -> Int
branchy k ys = case ys of
  [] -> k
  (a : rest)
    | a `mod` 3 == 0 -> let big = replicate (a `mod` 50) a in branchy (k + length big) rest
    | a `mod` 3 == 1 -> branchy (k + walk (build (a `mod` 20)) 0) rest
    | otherwise -> let t = show a in branchy (k + length t) rest

main :: IO ()
main = do
  let shared = map (* 3) [1 .. 2000 :: Int]
      total = sum shared
  r <- newIORef (0 :: Int)
  mapM_ (\x -> modifyIORef' r (+ (x + total `mod` 7))) shared
  readIORef r >>= print
  print (branchy 0 [1 .. 3000], walk (build 5000) 0)
  print (foldl' (\acc x -> acc + length (show (x, total))) 0 [1 .. 2000 :: Int])

import Data.Array
import Data.Array.ST
import Data.Array.IO
import Control.Monad
import Control.Monad.ST

sieve :: Int -> [Int]
sieve n = [i | (i, True) <- assocs arr]
  where
    arr = runSTUArray $ do
      a <- newArray (2, n) True
      forM_ [2 .. n] $ \i -> do
        isP <- readArray a i
        when (isP && i * i <= n) $ forM_ [i * i, i * i + i .. n] $ \j -> writeArray a j False
      return a

main :: IO ()
main = do
  let a = listArray (0, 4) "hello" :: Array Int Char
  print (a ! 1, bounds a, elems a)
  let b = accumArray (+) 0 (0, 3) [(i `mod` 4, i) | i <- [1 .. 10]] :: Array Int Int
  print b
  print (b // [(0, 100)], fmap (* 2) b)
  let grid = array ((0, 0), (2, 2)) [((i, j), i * 3 + j) | i <- [0 .. 2], j <- [0 .. 2]] :: Array (Int, Int) Int
  print (grid ! (1, 2), sum (elems grid), indices grid == range ((0, 0), (2, 2)))
  print (sieve 60)
  io <- newListArray (1, 5) [10, 20, 30, 40, 50] :: IO (IOUArray Int Int)
  forM_ [1 .. 5] $ \i -> readArray io i >>= writeArray io i . (+ i)
  getElems io >>= print

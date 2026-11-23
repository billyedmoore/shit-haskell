-- A sieve of Eratosthenes in an unboxed mutable array in ST: array primitives in a tight loop.
import Control.Monad
import Control.Monad.ST
import Data.Array.ST
import Data.Array.Unboxed

sieve :: Int -> UArray Int Bool
sieve n = runSTUArray $ do
  a <- newArray (2, n) True
  forM_ [2 .. n] $ \i -> do
    p <- readArray a i
    when (p && i * i <= n) $ forM_ [i * i, i * i + i .. n] $ \j -> writeArray a j False
  pure a

main :: IO ()
main = do
  let s = sieve 10000000
  print (length (filter id (elems s)))

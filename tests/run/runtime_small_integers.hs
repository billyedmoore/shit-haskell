module Main where
import Data.IORef
import Data.Word
import Control.Monad
import System.Mem

{-# NOINLINE bump #-}
bump :: Int -> Int
bump n = n + 1

main :: IO ()
main = do
  print [(n, bump n, n - 1) | n <- [-17, -16, -1, 0, 1, 254, 255, 256, minBound, maxBound]]
  print [(n + 1, n - 1) | n <- [0, 1, 255, 256, maxBound] :: [Word]]
  print (map (+ 0) [-17, -16, 0, 1, 255, 256, 12345678901234567890] :: [Integer])
  r <- newIORef (0 :: Int)
  replicateM_ 20000 (modifyIORef' r (\n -> bump n `mod` 257))
  performGC
  readIORef r >>= print
  let kept = map bump [-17, -1, 0, 254]
  print (sum kept)
  performGC
  print kept

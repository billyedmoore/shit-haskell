module Main where
import Data.IORef
import System.Mem

data Marker = A | B deriving (Eq, Ord, Enum, Bounded, Show)
data Box = Box Int deriving (Eq, Show)

{-# NOINLINE choose #-}
choose :: Int -> Marker
choose n = if even n then A else B

main :: IO ()
main = do
  print (map choose [0 .. 10])
  print ([minBound, maxBound] :: [Marker])
  print (choose 0 == A, choose 1 == B, A /= B)
  let held = map choose [1 .. 20000]
  r <- newIORef held
  print (length (filter (== A) held))
  performGC
  readIORef r >>= print . take 6
  print ([Box 1, Box 2, Box 1], Box 1 == Box 2)
  print (map (\n -> if n == 0 then [] else [()]) [0 .. 3])
  print ([True, False, choose 0 == choose 2], ())

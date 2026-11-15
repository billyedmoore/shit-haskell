module Main where
import Control.Exception

class Eq a => Mixed a where
  fast :: a -> Int
  fallback :: a -> Int
  fallback x = fast x + 100
  unused :: a -> Int
  unused _ = error "unused default"

data Value = Value Int deriving Eq
instance Mixed Value where
  fast (Value n) = n * 3

main :: IO ()
main = do
  print (fast (Value 7), fallback (Value 7))
  print (map (\n -> (n > (10 :: Int), max n 10, min n 10)) [minBound, 9, 10, 11, maxBound])
  print (sum [fast (Value n) | n <- [1 .. 1000]])
  failed <- try (evaluate (unused (Value 0))) :: IO (Either ErrorCall Int)
  print (either (const True) (const False) failed)
  print (fast (Value 8), fallback (Value 8))

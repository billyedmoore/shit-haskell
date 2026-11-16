module Main where
import Control.Exception

{-# NOINLINE arithmetic #-}
arithmetic :: Int -> Int -> IO ()
arithmetic a b = do
  print (a + b, a - b, a * b)
  print (a == b, a /= b, a < b, a <= b, a > b, a >= b)

{-# NOINLINE add #-}
add :: Int -> Int -> Int
add a b = a + b

main :: IO ()
main = do
  mapM_ (uncurry arithmetic) [(maxBound,1),(minBound,1),(maxBound,2),(minBound,maxBound),(0,0),(-1,1),(255,256)]
  x <- try (evaluate (add (error "left") 3)) :: IO (Either ErrorCall Int)
  print (either (const True) (const False) x)
  y <- try (evaluate (add 3 (error "right"))) :: IO (Either ErrorCall Int)
  print (either (const True) (const False) y)
  print (add 20 22)

{-# LANGUAGE BangPatterns #-}
module Main where
import Control.Exception

{-# NOINLINE loop #-}
loop :: Int -> Int -> Int
loop !acc n
  | n <= 0 = acc
  | otherwise = let a = acc; b = a; c = n; d = c
                in loop (b + d) (d - 1)

main :: IO ()
main = do
  print (loop 0 10000)
  let ones = 1 : ones :: [Int]
      alias = ones
  print (take 6 alias)
  let shared = sum [1 .. 1000 :: Int]
      a = shared
      b = a
      capture = \n -> b + n
  print (capture 7, capture 9, shared)
  lazyResult <- try (evaluate (let x = error "unused" :: Int; a = x in const 42 a)) :: IO (Either ErrorCall Int)
  print lazyResult
  strictResult <- try (evaluate (let x = error "forced" :: Int; a = x in a `seq` 42)) :: IO (Either ErrorCall Int)
  print (either (const True) (const False) strictResult)
  print (loop 0 10)

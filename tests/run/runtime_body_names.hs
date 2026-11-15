module Main where
import Control.Exception

{-# NOINLINE λtotal #-}
λtotal :: Int -> Int
λtotal n = sum [1 .. n]

{-# NOINLINE résumé #-}
résumé :: Int -> Int
résumé x = let α = λtotal x
               β = α + λtotal (x + 1)
           in β + α

main :: IO ()
main = do
  print (map résumé [10, 100, 1000])
  failed <- try (evaluate (résumé (error "name bytes"))) :: IO (Either ErrorCall Int)
  print (either (const True) (const False) failed)
  print (résumé 20)

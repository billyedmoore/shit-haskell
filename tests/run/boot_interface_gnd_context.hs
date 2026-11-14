module Main where
import CacheBootGndContextConsumer
import CacheBootGndContext
main :: IO ()
main = print (sameN (makeN (3 :: Int)) (makeN 3), sameN (makeN 'a') (makeN 'b'))

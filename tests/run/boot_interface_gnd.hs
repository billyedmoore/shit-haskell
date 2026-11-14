-- The boot instance discovers b before a; GND starts with the declaration's a,b order.
module Main where
import CacheBootGndConsumer
import CacheBootGnd
main :: IO ()
main = print (callN 'x' (makeN 17 :: N Bool Char))

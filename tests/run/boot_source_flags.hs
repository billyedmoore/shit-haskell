module Main where
import LookupFlagB
import LookupFlagA
main :: IO ()
main = print (through [2, 3, 5], value [11 :: Int])

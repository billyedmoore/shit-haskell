module Main where
import CacheBootOrphan ()
import CacheBootOrphanType
main :: IO ()
main = case (3 + 4 :: N) of N n -> print n

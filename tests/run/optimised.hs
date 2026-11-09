import Data.List (foldl')

main :: IO ()
main = print (foldl' (+) 0 [x * x `mod` 7 | x <- [1 .. 300000 :: Int]])

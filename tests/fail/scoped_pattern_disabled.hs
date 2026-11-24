{-# LANGUAGE NoScopedTypeVariables #-}

f :: Int -> Int
f (x :: Int) = x

main :: IO ()
main = print (f 1)

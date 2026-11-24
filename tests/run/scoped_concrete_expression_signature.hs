{-# LANGUAGE NoScopedTypeVariables #-}

f :: Int -> Int
f x = x :: Int

main :: IO ()
main = print (f 1)

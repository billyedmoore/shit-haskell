{-# LANGUAGE ScopedTypeVariables #-}

f :: a -> a
f x = x :: a

main :: IO ()
main = print (f False)

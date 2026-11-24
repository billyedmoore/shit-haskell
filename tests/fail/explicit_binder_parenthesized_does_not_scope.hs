{-# LANGUAGE ScopedTypeVariables #-}

f :: (forall a. a -> a)
f x = x :: a

main :: IO ()
main = print (f False)

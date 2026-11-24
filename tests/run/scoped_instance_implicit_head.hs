{-# LANGUAGE ScopedTypeVariables #-}

class C a where
  f :: a -> a
instance C [a] where
  f x = x :: [a]

main :: IO ()
main = print (f [False])

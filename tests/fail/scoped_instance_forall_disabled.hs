{-# LANGUAGE NoScopedTypeVariables #-}

class C a where
  f :: a -> a
instance forall a. C [a] where
  f x = x :: [a]

main :: IO ()
main = print (f [False])

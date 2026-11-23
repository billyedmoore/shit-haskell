{-# LANGUAGE NoScopedTypeVariables #-}

class C a where
  f :: a -> a
  f x = x :: a
instance C Bool

main :: IO ()
main = print (f False)

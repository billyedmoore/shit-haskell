{-# LANGUAGE ScopedTypeVariables #-}

class C a where
  f :: a -> b -> b
  f _ x = x :: b
instance C Bool

main :: IO ()
main = print (f False True)

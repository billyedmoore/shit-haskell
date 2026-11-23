{-# LANGUAGE DefaultSignatures, ScopedTypeVariables #-}

class C a where
  f :: a -> b -> (a, b)
  default f :: forall b. a -> b -> (a, b)
  f x y = (x :: a, y :: b)
instance C Bool

main :: IO ()
main = print (f False True)

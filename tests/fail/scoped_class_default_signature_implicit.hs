{-# LANGUAGE DefaultSignatures, ScopedTypeVariables #-}

class C a where
  f :: forall b. a -> b -> b
  default f :: a -> b -> b
  f _ x = x :: b
instance C Bool

main :: IO ()
main = print (f False True)

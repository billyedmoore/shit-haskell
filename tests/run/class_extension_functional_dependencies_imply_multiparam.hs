{-# LANGUAGE Haskell2010, FunctionalDependencies #-}

class Pair a b | a -> b where
  pair :: a -> b

instance Pair Bool Int where
  pair _ = 7

main :: IO ()
main = print (pair True)

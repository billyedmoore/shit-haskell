{-# LANGUAGE Haskell2010, PatternSignatures #-}

f :: a -> a
f (x :: a) = identity x
  where
    identity :: a -> a
    identity y = y

main :: IO ()
main = print (f False)

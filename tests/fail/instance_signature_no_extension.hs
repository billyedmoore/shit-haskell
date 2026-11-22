{-# LANGUAGE NoInstanceSigs #-}

class C a where
  value :: a -> a

instance C Int where
  value :: Int -> Int
  value x = x

main = pure ()

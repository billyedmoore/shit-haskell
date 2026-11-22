{-# LANGUAGE NoDefaultSignatures #-}

class C a where
  value :: a -> Int
  default value :: Show a => a -> Int
  value _ = 0

main = pure ()

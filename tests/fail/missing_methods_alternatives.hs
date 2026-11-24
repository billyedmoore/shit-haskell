{-# OPTIONS_GHC -Werror=missing-methods #-}

class Choose a where
  alpha, first, second, third :: a -> a
  first = third
  second = third
  third = first
  {-# MINIMAL alpha, ((first, second) | third) #-}

instance Choose Int where
  first = id

main :: IO ()
main = pure ()
